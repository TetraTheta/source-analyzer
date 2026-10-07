use std::{
  env, fs,
  path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::cli::{AnalysisOptions, CleanupOptions, SourceOptions};

pub(crate) fn resolve_analysis(options: &mut AnalysisOptions) -> Result<()> {
  let Some((profile, config_dir, name)) = load_selected(options.source.profile.take())? else { return Ok(()) };
  apply_source(&profile, &config_dir, &mut options.source)?;
  if !options.missing
    && !options.present
    && !options.all
    && let Some(selection) = profile.get("type").and_then(toml::Value::as_str)
  {
    match selection {
      "all" => options.all = true,
      "missing" => options.missing = true,
      "present" => options.present = true,
      _ => bail!("profile '{name}' has invalid type '{selection}'; expected all, missing, or present"),
    }
  }
  Ok(())
}

pub(crate) fn resolve_cleanup(target: &mut Option<PathBuf>, options: &mut CleanupOptions) -> Result<()> {
  let Some((profile, config_dir, _)) = load_selected(options.source.profile.take())? else { return Ok(()) };
  apply_source(&profile, &config_dir, &mut options.source)?;
  if target.is_none()
    && let Some(path) = profile.get("target").and_then(toml::Value::as_str).filter(|path| !path.is_empty())
  {
    *target = Some(resolve_path(&config_dir, path));
  }
  if options.keep.is_empty()
    && let Some(values) = profile.get("keep")
  {
    options.keep = values
      .as_array()
      .context("profile keep must be an array")?
      .iter()
      .map(|value| value.as_str().context("profile keep must contain strings").map(str::to_owned))
      .collect::<Result<_>>()?;
  }
  options.unused |= profile.get("unused").and_then(toml::Value::as_bool).unwrap_or(false);
  options.delete |= profile.get("delete").and_then(toml::Value::as_bool).unwrap_or(false);
  options.no_backup |= profile.get("no_backup").and_then(toml::Value::as_bool).unwrap_or(false);
  Ok(())
}

fn load_selected(requested: Option<String>) -> Result<Option<(toml::Table, PathBuf, String)>> {
  let Some(requested) = requested else { return Ok(None) };
  let executable = env::current_exe().context("failed to locate the source-analyzer executable")?;
  let config_path = executable.parent().context("source-analyzer executable has no parent directory")?.join("source-analyzer.toml");
  let text = fs::read_to_string(&config_path).with_context(|| format!("failed to read {}", config_path.display()))?;
  let config = text.parse::<toml::Table>().with_context(|| format!("failed to parse {}", config_path.display()))?;
  let presets = config.get("preset").and_then(toml::Value::as_table).context("source-analyzer.toml is missing [preset]")?;
  let name = if requested.is_empty() {
    presets.get("default").and_then(toml::Value::as_str).context("source-analyzer.toml is missing preset.default")?.to_owned()
  } else {
    requested
  };
  let profile = presets.get(&name).and_then(toml::Value::as_table).with_context(|| format!("profile '{name}' does not exist"))?.clone();
  Ok(Some((profile, config_path.parent().expect("config path has a parent").to_path_buf(), name)))
}

fn apply_source(profile: &toml::Table, config_dir: &Path, options: &mut SourceOptions) -> Result<()> {
  if options.gameinfo.is_none()
    && let Some(path) = profile.get("gameinfo").and_then(toml::Value::as_str).filter(|path| !path.is_empty())
  {
    options.gameinfo = Some(resolve_path(config_dir, path));
  }
  if options.base_dir.is_none()
    && let Some(path) = profile.get("base_dir").and_then(toml::Value::as_str).filter(|path| !path.is_empty())
  {
    options.base_dir = Some(resolve_path(config_dir, path));
  }
  if options.extra_base_dir.is_empty()
    && let Some(values) = profile.get("extra_base_dirs")
  {
    for value in values.as_array().context("profile extra_base_dirs must be an array")? {
      let value = value.as_str().filter(|value| !value.is_empty()).context("profile extra_base_dirs must contain non-empty strings")?;
      options.extra_base_dir.push(resolve_path(config_dir, value));
    }
  }
  options.verbose |= profile.get("verbose").and_then(toml::Value::as_bool).unwrap_or(false);
  Ok(())
}

fn resolve_path(config_dir: &Path, value: &str) -> PathBuf {
  let path = Path::new(value);
  if path.is_absolute() { path.to_path_buf() } else { config_dir.join(path) }
}

#[cfg(test)]
mod tests;
