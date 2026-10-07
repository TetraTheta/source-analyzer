use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anstream::println;
use anstyle::Style;
use anyhow::{Context, Result};

use crate::bsp;
use crate::model;
use crate::output;
use crate::source_fs::{AssetStatus, SourceFs, normalize_resource};

#[derive(Debug, Clone, Copy)]
pub enum Selection {
  Missing,
  Present,
  All,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub enum ResourceKind {
  MapMaterial,
  MapModel,
  ModelMaterial,
  MapSound,
  MapParticle,
}

#[derive(Debug, Clone)]
pub struct Dependency {
  kind: ResourceKind,
  path: String,
  source: Option<String>,
  status: AssetStatus,
}

pub fn analyze_model_file(path: &Path, source_fs: &SourceFs) -> Result<Vec<Dependency>> {
  let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
  analyze_model_bytes(&bytes, source_fs)
}

pub fn analyze_map(path: &Path, source_fs: &mut SourceFs, verbose: bool) -> Result<Vec<Dependency>> {
  let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
  let assets = bsp::parse(&bytes).context("failed to parse BSP")?;
  source_fs.set_embedded_pak(assets.pakfile)?;
  let mut report = Vec::new();

  for material in assets.materials {
    report.push(resolve_dependency(source_fs, ResourceKind::MapMaterial, material));
  }
  for sound in assets.sounds {
    report.push(resolve_dependency(source_fs, ResourceKind::MapSound, sound));
  }
  for particle in assets.particles {
    report.push(resolve_dependency(source_fs, ResourceKind::MapParticle, particle));
  }
  for model_path in assets.models {
    let resolved = source_fs.resolve(&model_path);
    let dependency = dependency_from_resolution(ResourceKind::MapModel, model_path.clone(), resolved.as_ref());
    report.push(dependency);
    let Some(resolved) = resolved else {
      if verbose {
        output::warning(format_args!("Skipping material analysis for missing model: {model_path}"));
      }
      continue;
    };
    let model_bytes = source_fs.read(&model_path, &resolved).with_context(|| format!("failed to read model {model_path}"))?;
    report.extend(analyze_model_bytes(&model_bytes, source_fs).with_context(|| format!("failed to analyze model {model_path}"))?);
  }
  Ok(deduplicate(report))
}

fn analyze_model_bytes(bytes: &[u8], source_fs: &SourceFs) -> Result<Vec<Dependency>> {
  let mut report = Vec::new();
  for candidates in model::material_candidates(bytes)? {
    let found = candidates.iter().find_map(|path| source_fs.resolve(path).map(|resolved| (path, resolved)));
    if let Some((path, resolved)) = found {
      report.push(dependency_from_resolution(ResourceKind::ModelMaterial, path.clone(), Some(&resolved)));
    } else {
      report.extend(candidates.into_iter().map(|path| Dependency {
        kind: ResourceKind::ModelMaterial,
        path,
        source: None,
        status: AssetStatus::Missing,
      }));
    }
  }
  Ok(deduplicate(report))
}

fn resolve_dependency(source_fs: &SourceFs, kind: ResourceKind, path: String) -> Dependency {
  let resolved = source_fs.resolve(&path);
  dependency_from_resolution(kind, path, resolved.as_ref())
}

fn dependency_from_resolution(kind: ResourceKind, path: String, resolved: Option<&crate::source_fs::ResolvedAsset>) -> Dependency {
  Dependency {
    kind,
    path: normalize_resource(&path),
    source: resolved.and_then(|asset| asset.source.clone()),
    status: resolved.map_or(AssetStatus::Missing, |asset| asset.status),
  }
}

fn deduplicate(dependencies: Vec<Dependency>) -> Vec<Dependency> {
  let mut unique = BTreeMap::new();
  for dependency in dependencies {
    unique.insert((dependency.kind, dependency.path.clone()), dependency);
  }
  unique.into_values().collect()
}

pub fn print_report(dependencies: &[Dependency], selection: Selection) {
  for status in [AssetStatus::Missing, AssetStatus::Embedded, AssetStatus::Present] {
    if !selection_allows(selection, status) {
      continue;
    }
    for kind in [ResourceKind::MapMaterial, ResourceKind::MapModel, ResourceKind::ModelMaterial, ResourceKind::MapSound, ResourceKind::MapParticle] {
      let group: Vec<_> = dependencies.iter().filter(|dependency| dependency.status == status && dependency.kind == kind).collect();
      if group.is_empty() {
        continue;
      }
      print_banner(status, kind);
      for dependency in group {
        let style = status_style(status);
        let source = dependency.source.as_ref().map(|source| format!("  [{source}]")).unwrap_or_default();
        println!("[{style}{:8}{style:#}] [{:8}] {}{}", status_name(status), type_name(kind), dependency.path, source);
      }
      println!();
    }
  }
}

fn selection_allows(selection: Selection, status: AssetStatus) -> bool {
  match selection {
    Selection::Missing => status == AssetStatus::Missing,
    Selection::Present => status != AssetStatus::Missing,
    Selection::All => true,
  }
}

fn print_banner(status: AssetStatus, kind: ResourceKind) {
  let style = status_style(status);
  println!("{style}# {} {}{style:#}\n", title_case(status_name(status)), title_case(category_name(kind)));
}

fn title_case(value: &str) -> String {
  value
    .split_whitespace()
    .map(|word| {
      let mut characters = word.chars();
      characters.next().map(|first| first.to_uppercase().chain(characters.flat_map(char::to_lowercase)).collect::<String>()).unwrap_or_default()
    })
    .collect::<Vec<_>>()
    .join(" ")
}

fn status_style(status: AssetStatus) -> Style {
  match status {
    AssetStatus::Missing => output::ERROR,
    AssetStatus::Embedded => output::EMBEDDED,
    AssetStatus::Present => output::PRESENT,
  }
}

fn status_name(status: AssetStatus) -> &'static str {
  match status {
    AssetStatus::Missing => "MISSING",
    AssetStatus::Embedded => "EMBEDDED",
    AssetStatus::Present => "PRESENT",
  }
}

fn category_name(kind: ResourceKind) -> &'static str {
  match kind {
    ResourceKind::MapMaterial => "MAP MATERIAL",
    ResourceKind::MapModel => "MAP MODEL",
    ResourceKind::ModelMaterial => "MODEL MATERIAL",
    ResourceKind::MapSound => "MAP SOUND",
    ResourceKind::MapParticle => "MAP PARTICLE",
  }
}

fn type_name(kind: ResourceKind) -> &'static str {
  match kind {
    ResourceKind::MapMaterial | ResourceKind::ModelMaterial => "MATERIAL",
    ResourceKind::MapModel => "MODEL",
    ResourceKind::MapSound => "SOUND",
    ResourceKind::MapParticle => "PARTICLE",
  }
}

#[cfg(test)]
mod tests;
