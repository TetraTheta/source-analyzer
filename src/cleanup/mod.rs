mod discovery;
mod material;

use std::{
  collections::{BTreeMap, BTreeSet, VecDeque},
  fs,
  path::{Path, PathBuf},
};

use anstream::println;
use anyhow::{Context, Result, bail};

use crate::{
  bsp, model, output,
  source_fs::{SourceFs, normalize_resource},
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Kind {
  Material,
  Model,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Reason {
  Duplicate,
  Unused,
}

#[derive(Debug)]
struct Candidate {
  file: PathBuf,
  kind: Kind,
  path: String,
  reason: Reason,
  source: Option<String>,
}

pub(crate) fn run(
  target: &Path,
  source_fs: &mut SourceFs,
  unused: bool,
  delete: bool,
  no_backup: bool,
  keep: &[String],
  verbose: bool,
) -> Result<()> {
  if !target.is_dir() {
    bail!("target directory does not exist: {}", target.display());
  }
  let files = discovery::target_files(target)?;
  let relative = files
    .iter()
    .map(|file| {
      file
        .strip_prefix(target)
        .map(|path| normalize_resource(&path.to_string_lossy()))
        .with_context(|| format!("{} is outside target directory", file.display()))
    })
    .collect::<Result<Vec<_>>>()?;
  let kept = relative.iter().filter(|path| discovery::kept(path, keep)).cloned().collect::<BTreeSet<_>>();
  let duplicates = duplicate_files(source_fs, &files, &relative, &kept)?;
  let used = if unused { Some(used_files(target, source_fs, keep, verbose)?) } else { None };
  let groups = model_groups(&relative);
  let mut candidates = Vec::new();

  for (file, path) in files.into_iter().zip(relative) {
    if kept.contains(&path) {
      continue;
    }
    let kind = kind(&path);
    let is_unused = used.as_ref().is_some_and(|used| !used.contains(&path) && group_unused(&path, &groups, used));
    if let Some(source) = duplicates.get(&path) {
      candidates.push(Candidate { file, kind, path, reason: Reason::Duplicate, source: source.clone() });
    } else if is_unused {
      candidates.push(Candidate { file, kind, path, reason: Reason::Unused, source: None });
    }
  }
  candidates.sort_by_key(|candidate| (candidate.reason, candidate.kind, candidate.path.clone()));
  print_report(&candidates);
  if delete {
    apply(target, &candidates, no_backup)?;
  }
  Ok(())
}

fn duplicate_files(
  source_fs: &SourceFs,
  files: &[PathBuf],
  relative: &[String],
  kept: &BTreeSet<String>,
) -> Result<BTreeMap<String, Option<String>>> {
  let mut duplicates = BTreeMap::new();
  for (file, path) in files.iter().zip(relative) {
    if kept.contains(path) {
      continue;
    }
    let Some(resolved) = source_fs.resolve(path) else { continue };
    let target_bytes = fs::read(file).with_context(|| format!("failed to read {}", file.display()))?;
    let source_bytes = source_fs.read(path, &resolved).with_context(|| format!("failed to read fallback {path}"))?;
    let equal = if path.ends_with(".vmt") {
      material::equivalent(&target_bytes, &source_bytes).with_context(|| {
        let source = resolved.source.as_deref().unwrap_or("resolved fallback");
        format!("failed to compare target VMT {} with {source}", file.display())
      })?
    } else {
      target_bytes == source_bytes
    };
    if equal {
      duplicates.insert(path.clone(), resolved.source.clone());
    }
  }

  let groups = model_groups(relative);
  for members in groups.values() {
    if members.iter().any(|path| !duplicates.contains_key(path)) {
      for path in members {
        duplicates.remove(path);
      }
    }
  }
  Ok(duplicates)
}

fn used_files(target: &Path, source_fs: &mut SourceFs, keep: &[String], verbose: bool) -> Result<BTreeSet<String>> {
  let maps = discovery::maps(target)?;
  if maps.is_empty() {
    bail!("--unused requires at least one maps/*.bsp file");
  }
  let mut all_used = BTreeSet::new();
  for map in maps {
    if verbose {
      output::info(format_args!("Analyzing cleanup dependencies: {}", map.display()));
    }
    let bytes = fs::read(&map).with_context(|| format!("failed to read {}", map.display()))?;
    let assets = bsp::parse(&bytes).with_context(|| format!("failed to parse {}", map.display()))?;
    source_fs.set_embedded_pak(assets.pakfile)?;
    let mut queue = VecDeque::new();
    queue.extend(assets.materials);
    queue.extend(assets.models);
    let mut used = BTreeSet::new();
    while let Some(path) = queue.pop_front() {
      let path = normalize_resource(&path);
      if !used.insert(path.clone()) {
        continue;
      }
      if discovery::kept(&path, keep) {
        if verbose {
          output::warning(format_args!("Skipping dependency inspection for kept resource: {path}"));
        }
        continue;
      }
      if path.ends_with(".vmt") {
        if let Some((bytes, source)) = read_resource(target, source_fs, &map, &path)? {
          queue.extend(material::dependencies(&bytes).with_context(|| format!("failed to inspect VMT {path} from {source}"))?);
        }
      } else if path.ends_with(".mdl")
        && let Some((bytes, source)) = read_resource(target, source_fs, &map, &path)?
      {
        for candidates in model::material_candidates(&bytes).with_context(|| format!("failed to inspect model {path} from {source}"))? {
          if let Some(candidate) = candidates.into_iter().find(|candidate| resource_exists(target, source_fs, candidate)) {
            queue.push_back(candidate);
          }
        }
        queue.extend(model_references(&bytes));
        let phy = format!("{}.phy", path.trim_end_matches(".mdl"));
        if let Some((bytes, _)) = read_resource(target, source_fs, &map, &phy)? {
          queue.extend(model_references(&bytes));
        }
      }
    }
    all_used.extend(used);
  }
  source_fs.set_embedded_pak(Vec::new())?;
  Ok(all_used)
}

fn read_resource(target: &Path, source_fs: &SourceFs, map: &Path, path: &str) -> Result<Option<(Vec<u8>, String)>> {
  let target_path = target.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
  if target_path.is_file() {
    let bytes = fs::read(&target_path).with_context(|| format!("failed to read {}", target_path.display()))?;
    return Ok(Some((bytes, target_path.display().to_string())));
  }
  let Some(resolved) = source_fs.resolve(path) else { return Ok(None) };
  let source = resolved.source.clone().unwrap_or_else(|| format!("BSP pakfile: {}", map.display()));
  source_fs.read(path, &resolved).map(|bytes| Some((bytes, source)))
}

fn resource_exists(target: &Path, source_fs: &SourceFs, path: &str) -> bool {
  target.join(path.replace('/', std::path::MAIN_SEPARATOR_STR)).is_file() || source_fs.resolve(path).is_some()
}

fn model_references(bytes: &[u8]) -> Vec<String> {
  bytes
    .split(|byte| !byte.is_ascii_graphic())
    .filter_map(|value| std::str::from_utf8(value).ok())
    .filter(|value| value.to_ascii_lowercase().ends_with(".mdl"))
    .map(|value| normalize_resource(value.trim_matches(['"', '\'', '/', '\\'])))
    .filter(|value| value.starts_with("models/"))
    .collect()
}

fn model_groups(paths: &[String]) -> BTreeMap<String, Vec<String>> {
  let mut groups = BTreeMap::new();
  for path in paths {
    if let Some(group) = discovery::model_group(path) {
      groups.entry(group).or_insert_with(Vec::new).push(path.clone());
    }
  }
  groups
}

fn group_unused(path: &str, groups: &BTreeMap<String, Vec<String>>, used: &BTreeSet<String>) -> bool {
  let Some(group) = discovery::model_group(path) else { return true };
  !groups.get(&group).is_some_and(|members| members.iter().any(|member| used.contains(member))) && !used.contains(&format!("{group}.mdl"))
}

fn kind(path: &str) -> Kind {
  if path.starts_with("materials/") { Kind::Material } else { Kind::Model }
}

fn apply(target: &Path, candidates: &[Candidate], no_backup: bool) -> Result<()> {
  if !no_backup {
    for candidate in candidates {
      let backup = backup_path(target, &candidate.path);
      if backup.exists() {
        bail!("backup already exists: {}", backup.display());
      }
    }
  }
  for candidate in candidates {
    if no_backup {
      fs::remove_file(&candidate.file).with_context(|| format!("failed to delete {}", candidate.file.display()))?;
    } else {
      let backup = backup_path(target, &candidate.path);
      fs::create_dir_all(backup.parent().expect("backup path has a parent"))?;
      fs::rename(&candidate.file, &backup).with_context(|| format!("failed to move {} to {}", candidate.file.display(), backup.display()))?;
    }
  }
  Ok(())
}

fn backup_path(target: &Path, path: &str) -> PathBuf {
  let (root, relative) = path.split_once('/').expect("cleanup paths have a root directory");
  target.join(format!(".{root}")).join(relative.replace('/', std::path::MAIN_SEPARATOR_STR))
}

fn print_report(candidates: &[Candidate]) {
  for reason in [Reason::Duplicate, Reason::Unused] {
    for kind in [Kind::Material, Kind::Model] {
      let group = candidates.iter().filter(|candidate| candidate.reason == reason && candidate.kind == kind).collect::<Vec<_>>();
      if group.is_empty() {
        continue;
      }
      let title = match (reason, kind) {
        (Reason::Duplicate, Kind::Material) => "Duplicate Material",
        (Reason::Duplicate, Kind::Model) => "Duplicate Model",
        (Reason::Unused, Kind::Material) => "Unused Material",
        (Reason::Unused, Kind::Model) => "Unused Model",
      };
      let style = output::PRESENT;
      println!("{style}# {title}{style:#}\n");
      for candidate in group {
        let status = match reason {
          Reason::Duplicate => "DUPLICATE",
          Reason::Unused => "UNUSED",
        };
        let kind = match kind {
          Kind::Material => "MATERIAL",
          Kind::Model => "MODEL",
        };
        let source = candidate.source.as_ref().map(|source| format!("  [{source}]")).unwrap_or_default();
        println!("[{:9}] [{kind:8}] {}{source}", status, candidate.path);
      }
      println!();
    }
  }
}

#[cfg(test)]
mod tests;
