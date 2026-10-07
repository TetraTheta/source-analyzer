use std::{
  collections::HashSet,
  fs,
  path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use kva::text::Parser;
use source_vpk::Vpk;

use super::{SearchRoot, canonical_path_key, find_compound, find_value, path_key};
use crate::output;

pub(super) fn add_mounts(
  gameinfo_dir: &Path,
  roots: &mut Vec<SearchRoot>,
  seen: &mut HashSet<String>,
  excluded_root: Option<&str>,
  verbose: bool,
) -> Result<()> {
  let cfg_dir = gameinfo_dir.join("cfg");
  let mount_cfg = cfg_dir.join("mount.cfg");
  if mount_cfg.is_file() {
    let text = fs::read_to_string(&mount_cfg).with_context(|| format!("failed to read {}", mount_cfg.display()))?;
    let root = Parser::new(&text).numeric_inference(false).parse().context("failed to parse mount.cfg as Valve KeyValues1")?;
    for entry in find_compound(&root, "mountcfg")? {
      if let Some(value) = entry.data.as_str() {
        let path = Path::new(value);
        let path = if path.is_absolute() { path.to_path_buf() } else { gameinfo_dir.join(path) };
        add_content(&path, roots, seen, excluded_root, verbose)?;
      }
    }
  }

  let depot_cfg = cfg_dir.join("mountdepots.txt");
  let depots: Vec<String> = if depot_cfg.is_file() {
    let text = fs::read_to_string(&depot_cfg).with_context(|| format!("failed to read {}", depot_cfg.display()))?;
    let root = Parser::new(&text).numeric_inference(false).parse().context("failed to parse mountdepots.txt as Valve KeyValues1")?;
    find_compound(&root, "gamedepotsystem")?
      .iter()
      .filter(|entry| entry.data.as_str().is_some_and(|value| value != "0"))
      .map(|entry| entry.name.to_string())
      .collect()
  } else {
    ["cstrike", "hl1", "hl1_hd", "hl2", "hl2mp", "episodic", "ep2", "lostcoast"].into_iter().map(str::to_owned).collect()
  };

  for steamapps in steam_library_dirs(gameinfo_dir)? {
    for depot in &depots {
      let Some(app_id) = depot_app_id(depot) else {
        if verbose {
          output::warning(format_args!("Skipping unknown GMod depot: {depot}"));
        }
        continue;
      };
      let manifest = steamapps.join(format!("appmanifest_{app_id}.acf"));
      if !manifest.is_file() {
        continue;
      }
      let text = fs::read_to_string(&manifest).with_context(|| format!("failed to read {}", manifest.display()))?;
      let root = Parser::new(&text).numeric_inference(false).parse().with_context(|| format!("failed to parse {}", manifest.display()))?;
      let Some(install_dir) = find_value(find_compound(&root, "AppState")?, "installdir") else { continue };
      add_content(&steamapps.join("common").join(install_dir).join(depot), roots, seen, excluded_root, verbose)?;
    }
  }
  Ok(())
}

fn depot_app_id(depot: &str) -> Option<&'static str> {
  match depot.to_ascii_lowercase().as_str() {
    "hl2" => Some("220"),
    "cstrike" => Some("240"),
    "hl1" | "hl1_hd" => Some("280"),
    "hl2mp" => Some("320"),
    "lostcoast" => Some("340"),
    "episodic" => Some("380"),
    "ep2" => Some("420"),
    "tf" => Some("440"),
    _ => None,
  }
}

fn add_content(path: &Path, roots: &mut Vec<SearchRoot>, seen: &mut HashSet<String>, excluded_root: Option<&str>, verbose: bool) -> Result<()> {
  if !path.is_dir() || excluded_root.is_some_and(|excluded| canonical_path_key(path) == excluded) {
    return Ok(());
  }
  if seen.insert(canonical_path_key(path)) {
    if verbose {
      output::info(format_args!("Mounted GMod content path: {}", path.display()));
    }
    roots.push(SearchRoot::Loose(path.to_path_buf()));
  }

  let mut vpks = fs::read_dir(path)?
    .flatten()
    .map(|entry| entry.path())
    .filter(|path| path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.to_ascii_lowercase().ends_with("_dir.vpk")))
    .collect::<Vec<_>>();
  vpks.sort_by_key(|path| path_key(path));
  for path in vpks {
    let archive = Vpk::open(&path).with_context(|| format!("failed to open VPK {}", path.display()))?;
    let actual_path = archive.path().to_path_buf();
    if seen.insert(canonical_path_key(&actual_path)) {
      if verbose {
        output::info(format_args!("Mounted GMod VPK: {} ({} entries)", actual_path.display(), archive.len()));
      }
      roots.push(SearchRoot::Vpk { path: actual_path, archive: Box::new(archive) });
    }
  }
  Ok(())
}

fn steam_library_dirs(gameinfo_dir: &Path) -> Result<Vec<PathBuf>> {
  let Some(steamapps) = gameinfo_dir.ancestors().find(|path| path.file_name().is_some_and(|name| name.eq_ignore_ascii_case("steamapps"))) else {
    return Ok(Vec::new());
  };
  let mut libraries = vec![steamapps.to_path_buf()];
  let library_file = steamapps.join("libraryfolders.vdf");
  if library_file.is_file() {
    let text = fs::read_to_string(&library_file).with_context(|| format!("failed to read {}", library_file.display()))?;
    let root = Parser::new(&text).numeric_inference(false).parse().context("failed to parse libraryfolders.vdf as Valve KeyValues1")?;
    for entry in find_compound(&root, "libraryfolders")? {
      let value = entry.data.as_str().or_else(|| entry.data.as_compound().and_then(|entries| find_value(entries, "path")));
      if let Some(value) = value {
        let path = PathBuf::from(value).join("steamapps");
        if path.is_dir() && !libraries.iter().any(|library| canonical_path_key(library) == canonical_path_key(&path)) {
          libraries.push(path);
        }
      }
    }
  }
  Ok(libraries)
}

#[cfg(test)]
mod tests;
