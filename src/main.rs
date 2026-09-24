mod analysis;
mod bsp;
mod model;
mod source_fs;

use std::{
  env, fs,
  path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::{ArgGroup, Args, Parser, Subcommand};

use analysis::{Selection, analyze_map, analyze_model_file, print_report};
use source_fs::SourceFs;

#[derive(Debug, Parser)]
#[command(version, about = "Analyze Source Engine map and model dependencies")]
struct Cli {
  #[command(subcommand)]
  command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
  /// Analyze a Source Engine BSP map.
  Map {
    /// BSP file to analyze.
    bsp: PathBuf,
    #[command(flatten)]
    options: Options,
  },
  /// Analyze a Source Engine MDL model.
  Model {
    /// MDL file to analyze.
    mdl: PathBuf,
    #[command(flatten)]
    options: Options,
  },
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("selection")
        .args(["missing", "present", "all"])
        .multiple(false)
))]
struct Options {
  /// Load a profile from source-analyzer.toml. Uses preset.default when NAME is omitted.
  #[arg(short = 'p', long, num_args = 0..=1, default_missing_value = "")]
  profile: Option<String>,

  /// Path to the game's gameinfo.txt.
  #[arg(short, long)]
  gameinfo: Option<PathBuf>,

  /// Source engine base directory. Inferred as gameinfo.txt/../.. when omitted.
  #[arg(long)]
  base_dir: Option<PathBuf>,

  /// Print only missing dependencies.
  #[arg(long)]
  missing: bool,

  /// Print only present and embedded dependencies.
  #[arg(long)]
  present: bool,

  /// Print every dependency. This is the default.
  #[arg(long)]
  all: bool,

  /// Print search-path and analysis progress to stderr.
  #[arg(short, long)]
  verbose: bool,
}

fn main() {
  if let Err(error) = run() {
    eprintln!("error: {error:#}");
    std::process::exit(1);
  }
}

fn run() -> Result<()> {
  let cli = Cli::parse();
  match cli.command {
    Command::Map { bsp, mut options } => {
      resolve_profile(&mut options)?;
      let gameinfo = options.gameinfo.as_deref().context("--gameinfo is required unless the selected profile provides it")?;
      let mut source_fs = SourceFs::from_gameinfo(gameinfo, options.base_dir.as_deref(), options.verbose)?;
      let report = analyze_map(&bsp, &mut source_fs, options.verbose)?;
      print_report(&report, selection(&options));
    },
    Command::Model { mdl, mut options } => {
      resolve_profile(&mut options)?;
      let gameinfo = options.gameinfo.as_deref().context("--gameinfo is required unless the selected profile provides it")?;
      let source_fs = SourceFs::from_gameinfo(gameinfo, options.base_dir.as_deref(), options.verbose)?;
      let report = analyze_model_file(&mdl, &source_fs)?;
      print_report(&report, selection(&options));
    },
  }
  Ok(())
}

fn resolve_profile(options: &mut Options) -> Result<()> {
  let Some(requested) = options.profile.take() else {
    return Ok(());
  };
  let executable = env::current_exe().context("failed to locate the source-analyzer executable")?;
  let config_path = executable.parent().context("source-analyzer executable has no parent directory")?.join("source-analyzer.toml");
  let text = fs::read_to_string(&config_path).with_context(|| format!("failed to read {}", config_path.display()))?;
  let config = text.parse::<toml::Table>().with_context(|| format!("failed to parse {}", config_path.display()))?;
  apply_profile(&config, &requested, config_path.parent().expect("config path has a parent"), options)
}

fn apply_profile(config: &toml::Table, requested: &str, config_dir: &Path, options: &mut Options) -> Result<()> {
  let presets = config.get("preset").and_then(toml::Value::as_table).context("source-analyzer.toml is missing [preset]")?;
  let name = if requested.is_empty() {
    presets.get("default").and_then(toml::Value::as_str).context("source-analyzer.toml is missing preset.default")?
  } else {
    requested
  };
  let profile = presets.get(name).and_then(toml::Value::as_table).with_context(|| format!("profile '{name}' does not exist"))?;

  if options.gameinfo.is_none()
    && let Some(path) = profile.get("gameinfo").and_then(toml::Value::as_str).filter(|path| !path.is_empty())
  {
    options.gameinfo = Some(resolve_config_path(config_dir, path));
  }
  if options.base_dir.is_none()
    && let Some(path) = profile.get("base_dir").and_then(toml::Value::as_str).filter(|path| !path.is_empty())
  {
    options.base_dir = Some(resolve_config_path(config_dir, path));
  }
  options.verbose |= profile.get("verbose").and_then(toml::Value::as_bool).unwrap_or(false);

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

fn resolve_config_path(config_dir: &Path, path: &str) -> PathBuf {
  let path = Path::new(path);
  if path.is_absolute() { path.to_path_buf() } else { config_dir.join(path) }
}

fn selection(options: &Options) -> Selection {
  if options.missing {
    Selection::Missing
  } else if options.present {
    Selection::Present
  } else {
    Selection::All
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_profile_fills_options_without_overriding_cli() {
    let cli = Cli::try_parse_from(["source-analyzer", "map", "test.bsp", "-p", "--gameinfo", "cli/gameinfo.txt"]).expect("CLI should parse");
    let Command::Map { mut options, .. } = cli.command else { unreachable!() };
    let config = r#"
[preset]
default = "ez2"

[preset.ez2]
gameinfo = "game/ez2/gameinfo.txt"
type = "missing"
verbose = true
"#
    .parse::<toml::Table>()
    .expect("TOML should parse");

    let requested = options.profile.take().expect("profile flag");
    apply_profile(&config, &requested, Path::new("config"), &mut options).expect("profile should apply");

    assert_eq!(options.gameinfo, Some(PathBuf::from("cli/gameinfo.txt")));
    assert!(options.missing);
    assert!(options.verbose);
  }
}
