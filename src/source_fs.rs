use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use kva::{KvEntry, text::Parser};
use source_vpk::Vpk;
use zip::ZipArchive;

use crate::output;

const MAX_ASSET_SIZE: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AssetStatus {
  Missing,
  Embedded,
  Present,
}

#[derive(Debug, Clone)]
pub struct ResolvedAsset {
  location: Location,
  pub source: Option<String>,
  pub status: AssetStatus,
}

#[derive(Debug, Clone)]
enum Location {
  Embedded,
  Loose(PathBuf),
  Vpk(usize),
}

#[derive(Debug)]
enum SearchRoot {
  Loose(PathBuf),
  Vpk { path: PathBuf, archive: Box<Vpk> },
}

#[derive(Debug)]
struct EmbeddedPak {
  bytes: Vec<u8>,
  names: HashMap<String, String>,
}

#[derive(Debug)]
pub struct SourceFs {
  embedded: Option<EmbeddedPak>,
  roots: Vec<SearchRoot>,
  verbose: bool,
}

impl SourceFs {
  pub fn from_gameinfo(gameinfo_path: &Path, base_dir: Option<&Path>, verbose: bool) -> Result<Self> {
    let text = fs::read_to_string(gameinfo_path).with_context(|| format!("failed to read {}", gameinfo_path.display()))?;
    let root = Parser::new(&text).numeric_inference(false).parse().context("failed to parse gameinfo.txt as Valve KeyValues1")?;
    let file_system = find_compound(&root, "FileSystem")?;
    let search_paths = find_child_compound(file_system, "SearchPaths")?;
    let gameinfo_dir = gameinfo_path.parent().context("gameinfo.txt has no parent directory")?;
    let inferred_base;
    let base_dir = if let Some(base_dir) = base_dir {
      base_dir
    } else {
      inferred_base = gameinfo_dir.parent().context("cannot infer Source base directory; pass --base-dir")?.to_path_buf();
      if verbose {
        output::info(format_args!("Using inferred Source base directory: {}", inferred_base.display()));
      }
      &inferred_base
    };

    let mut roots = Vec::new();
    let mut seen = HashSet::new();
    for entry in search_paths {
      if !entry.name.split('+').any(|id| id.trim().eq_ignore_ascii_case("game")) {
        continue;
      }
      let Some(value) = entry.data.as_str() else {
        continue;
      };
      let candidates = resolve_search_path(value, gameinfo_dir, base_dir)?;
      for candidate in candidates {
        if candidate.is_dir() {
          let key = path_key(&candidate);
          if seen.insert(key) {
            if verbose {
              output::info(format_args!("Mounted loose path: {}", candidate.display()));
            }
            roots.push(SearchRoot::Loose(candidate));
          }
          continue;
        }

        let Some(vpk_path) = resolve_vpk_path(&candidate) else {
          if verbose {
            output::warning(format_args!("Skipping missing search path: {}", candidate.display()));
          }
          continue;
        };
        let archive = Vpk::open(&vpk_path).with_context(|| format!("failed to open VPK {}", vpk_path.display()))?;
        let actual_path = archive.path().to_path_buf();
        if seen.insert(path_key(&actual_path)) {
          if verbose {
            output::info(format_args!("Mounted VPK: {} ({} entries)", actual_path.display(), archive.len()));
          }
          roots.push(SearchRoot::Vpk { path: actual_path, archive: Box::new(archive) });
        }
      }
    }

    Ok(Self { embedded: None, roots, verbose })
  }

  pub fn set_embedded_pak(&mut self, bytes: Vec<u8>) -> Result<()> {
    if bytes.is_empty() {
      return Ok(());
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes.as_slice())).context("failed to parse BSP embedded pakfile")?;
    let mut names = HashMap::new();
    for index in 0..archive.len() {
      let file = archive.by_index(index).context("failed to read BSP pakfile directory")?;
      if !file.is_dir() {
        names.entry(normalize_resource(file.name())).or_insert_with(|| file.name().to_owned());
      }
    }
    if self.verbose {
      output::info(format_args!("Mounted BSP pakfile: {} entries", names.len()));
    }
    self.embedded = Some(EmbeddedPak { bytes, names });
    Ok(())
  }

  pub fn resolve(&self, resource: &str) -> Option<ResolvedAsset> {
    let normalized = normalize_resource(resource);
    if self.embedded.as_ref().is_some_and(|pak| pak.names.contains_key(&normalized)) {
      return Some(ResolvedAsset { location: Location::Embedded, source: None, status: AssetStatus::Embedded });
    }

    for (index, root) in self.roots.iter().enumerate() {
      match root {
        SearchRoot::Loose(path) => {
          let full_path = path.join(resource.replace('/', std::path::MAIN_SEPARATOR_STR));
          if full_path.is_file() {
            return Some(ResolvedAsset {
              location: Location::Loose(full_path),
              source: Some(format!("loose: {}", path.display())),
              status: AssetStatus::Present,
            });
          }
        },
        SearchRoot::Vpk { path, archive } if archive.contains(&normalized) => {
          let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("unknown.vpk");
          return Some(ResolvedAsset { location: Location::Vpk(index), source: Some(format!("vpk: {name}")), status: AssetStatus::Present });
        },
        SearchRoot::Vpk { .. } => {},
      }
    }
    None
  }

  pub fn read(&self, resource: &str, resolved: &ResolvedAsset) -> Result<Vec<u8>> {
    match &resolved.location {
      Location::Embedded => {
        let pak = self.embedded.as_ref().context("BSP pakfile is not mounted")?;
        let normalized = normalize_resource(resource);
        let name = pak.names.get(&normalized).with_context(|| format!("embedded asset disappeared: {resource}"))?;
        let mut archive = ZipArchive::new(Cursor::new(pak.bytes.as_slice()))?;
        let mut file = archive.by_name(name)?;
        if file.size() > MAX_ASSET_SIZE {
          bail!("embedded asset is too large: {resource}");
        }
        let mut bytes = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
      },
      Location::Loose(path) => read_limited(path),
      Location::Vpk(index) => {
        let SearchRoot::Vpk { archive, .. } = &self.roots[*index] else {
          bail!("invalid VPK search root");
        };
        let entry = archive.entry(resource).with_context(|| format!("VPK asset disappeared: {resource}"))?;
        if entry.len() > MAX_ASSET_SIZE {
          bail!("VPK asset is too large: {resource}");
        }
        archive.read(resource).map_err(Into::into)
      },
    }
  }
}

pub fn normalize_resource(path: &str) -> String {
  path.replace('\\', "/").trim_start_matches('/').to_ascii_lowercase()
}

fn read_limited(path: &Path) -> Result<Vec<u8>> {
  let metadata = fs::metadata(path)?;
  if metadata.len() > MAX_ASSET_SIZE {
    bail!("asset is too large: {}", path.display());
  }
  fs::read(path).with_context(|| format!("failed to read {}", path.display()))
}

fn find_compound<'a>(entry: &'a KvEntry<'a>, name: &str) -> Result<&'a [KvEntry<'a>]> {
  let entries = entry.data.as_compound().with_context(|| format!("{} is not a KeyValues object", entry.name))?;
  find_child_compound(entries, name)
}

fn find_child_compound<'a>(entries: &'a [KvEntry<'a>], name: &str) -> Result<&'a [KvEntry<'a>]> {
  entries
    .iter()
    .find(|child| child.name.eq_ignore_ascii_case(name))
    .and_then(|child| child.data.as_compound())
    .with_context(|| format!("gameinfo.txt is missing {name}"))
}

fn resolve_search_path(value: &str, gameinfo_dir: &Path, base_dir: &Path) -> Result<Vec<PathBuf>> {
  const GAMEINFO: &str = "|gameinfo_path|";
  const ALL_SOURCE: &str = "|all_source_engine_paths|";
  let (base, relative) = if starts_with_ignore_ascii_case(value, GAMEINFO) {
    (gameinfo_dir, &value[GAMEINFO.len()..])
  } else if starts_with_ignore_ascii_case(value, ALL_SOURCE) {
    (base_dir, &value[ALL_SOURCE.len()..])
  } else {
    (base_dir, value)
  };
  expand_wildcards(base, relative)
}

fn expand_wildcards(base: &Path, relative: &str) -> Result<Vec<PathBuf>> {
  let components: Vec<_> =
    relative.trim_start_matches(['/', '\\']).split(['/', '\\']).filter(|component| !component.is_empty() && *component != ".").collect();
  let mut paths = vec![base.to_path_buf()];
  for component in components {
    if component == ".." {
      for path in &mut paths {
        path.pop();
      }
      continue;
    }
    if !component.contains(['*', '?']) {
      for path in &mut paths {
        path.push(component);
      }
      continue;
    }
    let mut expanded = Vec::new();
    for path in paths {
      let Ok(entries) = fs::read_dir(&path) else {
        continue;
      };
      for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if wildcard_matches(component, &name) {
          expanded.push(entry.path());
        }
      }
    }
    expanded.sort_by_key(|path| path_key(path));
    paths = expanded;
  }
  Ok(paths.into_iter().filter(|path| path.is_dir() || is_vpk_path(path)).collect())
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
  let pattern = pattern.as_bytes();
  let value = value.as_bytes();
  let (mut p, mut v, mut star, mut retry) = (0, 0, None, 0);
  while v < value.len() {
    if p < pattern.len() && (pattern[p] == b'?' || pattern[p].eq_ignore_ascii_case(&value[v])) {
      p += 1;
      v += 1;
    } else if p < pattern.len() && pattern[p] == b'*' {
      star = Some(p);
      p += 1;
      retry = v;
    } else if let Some(star_index) = star {
      p = star_index + 1;
      retry += 1;
      v = retry;
    } else {
      return false;
    }
  }
  while p < pattern.len() && pattern[p] == b'*' {
    p += 1;
  }
  p == pattern.len()
}

fn resolve_vpk_path(path: &Path) -> Option<PathBuf> {
  if path.is_file() {
    return Some(path.to_path_buf());
  }
  if !is_vpk_path(path) {
    return None;
  }
  let stem = path.file_stem()?.to_string_lossy();
  let dir_path = path.with_file_name(format!("{stem}_dir.vpk"));
  dir_path.is_file().then_some(dir_path)
}

fn is_vpk_path(path: &Path) -> bool {
  path.extension().and_then(|extension| extension.to_str()).is_some_and(|extension| extension.eq_ignore_ascii_case("vpk"))
}

fn starts_with_ignore_ascii_case(value: &str, prefix: &str) -> bool {
  value.get(..prefix.len()).is_some_and(|start| start.eq_ignore_ascii_case(prefix))
}

fn path_key(path: &Path) -> String {
  path.to_string_lossy().replace('\\', "/").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
  use super::wildcard_matches;

  #[test]
  fn wildcard_matching_is_case_insensitive() {
    assert!(wildcard_matches("*.VPK", "pak01_dir.vpk"));
    assert!(wildcard_matches("addon_?", "ADDON_1"));
    assert!(!wildcard_matches("*.vpk", "materials"));
  }
}
