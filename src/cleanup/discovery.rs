use std::{
  fs,
  path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use crate::source_fs::{normalize_resource, wildcard_matches};

pub(super) fn maps(target: &Path) -> Result<Vec<PathBuf>> {
  let directory = target.join("maps");
  let Ok(entries) = fs::read_dir(&directory) else { return Ok(Vec::new()) };
  let mut paths = entries
    .filter_map(Result::ok)
    .map(|entry| entry.path())
    .filter(|path| path.is_file() && extension(path).as_deref() == Some("bsp"))
    .collect::<Vec<_>>();
  paths.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());
  Ok(paths)
}

pub(super) fn target_files(target: &Path) -> Result<Vec<PathBuf>> {
  let mut files = Vec::new();
  collect(&target.join("materials"), &mut files)?;
  collect(&target.join("models"), &mut files)?;
  files.retain(|path| supported(path));
  files.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());
  Ok(files)
}

pub(super) fn kept(relative: &str, patterns: &[String]) -> bool {
  patterns.iter().any(|pattern| wildcard_matches(&normalize_resource(pattern), relative))
}

pub(super) fn model_group(path: &str) -> Option<String> {
  let lower = path.to_ascii_lowercase();
  for suffix in [".dx90.vtx", ".dx80.vtx", ".sw.vtx", ".vtx", ".mdl", ".vvd", ".phy", ".ani"] {
    if let Some(base) = lower.strip_suffix(suffix) {
      return Some(base.to_owned());
    }
  }
  None
}

fn collect(directory: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
  let Ok(entries) = fs::read_dir(directory) else { return Ok(()) };
  for entry in entries {
    let entry = entry.with_context(|| format!("failed to inspect {}", directory.display()))?;
    let file_type = entry.file_type().with_context(|| format!("failed to inspect {}", entry.path().display()))?;
    if file_type.is_symlink() {
      continue;
    }
    if file_type.is_dir() {
      collect(&entry.path(), output)?;
    } else if file_type.is_file() {
      output.push(entry.path());
    }
  }
  Ok(())
}

fn supported(path: &Path) -> bool {
  matches!(extension(path).as_deref(), Some("vmt" | "vtf" | "mdl" | "vvd" | "vtx" | "phy" | "ani"))
}

fn extension(path: &Path) -> Option<String> {
  path.extension()?.to_str().map(str::to_ascii_lowercase)
}
