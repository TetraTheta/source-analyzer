use std::path::PathBuf;

use clap::{ArgGroup, Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about = "Analyze and clean Source Engine map and model dependencies")]
pub(crate) struct Cli {
  #[command(subcommand)]
  pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
  /// Analyze a Source Engine BSP map.
  Map {
    /// BSP file to analyze.
    bsp: PathBuf,
    #[command(flatten)]
    options: AnalysisOptions,
  },
  /// Analyze a Source Engine MDL model.
  Model {
    /// MDL file to analyze.
    mdl: PathBuf,
    #[command(flatten)]
    options: AnalysisOptions,
  },
  /// Find redundant or map-unused loose materials and models.
  Clean {
    /// Mod or addon game directory. May be supplied by the selected profile.
    target: Option<PathBuf>,
    #[command(flatten)]
    options: CleanupOptions,
  },
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("selection").args(["missing", "present", "all"]).multiple(false)))]
pub(crate) struct AnalysisOptions {
  #[command(flatten)]
  pub(crate) source: SourceOptions,
  /// Print only missing dependencies.
  #[arg(long)]
  pub(crate) missing: bool,
  /// Print only present and embedded dependencies.
  #[arg(long)]
  pub(crate) present: bool,
  /// Print every dependency. This is the default.
  #[arg(long)]
  pub(crate) all: bool,
}

#[derive(Debug, Args)]
pub(crate) struct CleanupOptions {
  #[command(flatten)]
  pub(crate) source: SourceOptions,
  /// Also select files unused by every maps/*.bsp map. Runtime code references are not analyzed.
  #[arg(long)]
  pub(crate) unused: bool,
  /// Apply the cleanup. Files are moved to .materials and .models backup directories by default.
  #[arg(long)]
  pub(crate) delete: bool,
  /// Permanently delete selected files instead of backing them up. Requires --delete or an equivalent profile setting.
  #[arg(long)]
  pub(crate) no_backup: bool,
  /// Keep matching paths and skip their dependency inspection. May be repeated.
  #[arg(long)]
  pub(crate) keep: Vec<String>,
}

#[derive(Debug, Args)]
pub(crate) struct SourceOptions {
  /// Load a profile from source-analyzer.toml. Uses preset.default when NAME is omitted.
  #[arg(short = 'p', long, num_args = 0..=1, default_missing_value = "")]
  pub(crate) profile: Option<String>,
  /// Path to the game's gameinfo.txt.
  #[arg(short, long)]
  pub(crate) gameinfo: Option<PathBuf>,
  /// Source engine base directory. Inferred as gameinfo.txt/../.. when omitted.
  #[arg(long)]
  pub(crate) base_dir: Option<PathBuf>,
  /// Additional Source engine base and loose content directory. May be repeated.
  #[arg(short = 'B', long)]
  pub(crate) extra_base_dir: Vec<PathBuf>,
  /// Print search-path and analysis progress to stderr.
  #[arg(short, long)]
  pub(crate) verbose: bool,
}

#[cfg(test)]
mod tests;
