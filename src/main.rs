mod analysis;
mod bsp;
mod model;
mod source_fs;

use std::path::PathBuf;

use anyhow::Result;
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
  /// Path to the game's gameinfo.txt.
  #[arg(short, long)]
  gameinfo: PathBuf,

  /// Source engine base directory. Inferred as gameinfo.txt/../.. when omitted.
  #[arg(long)]
  base_dir: Option<PathBuf>,

  /// Print only missing dependencies.
  #[arg(short, long)]
  missing: bool,

  /// Print only present and embedded dependencies.
  #[arg(short, long)]
  present: bool,

  /// Print every dependency. This is the default.
  #[arg(short, long)]
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
    Command::Map { bsp, options } => {
      let mut source_fs = SourceFs::from_gameinfo(&options.gameinfo, options.base_dir.as_deref(), options.verbose)?;
      let report = analyze_map(&bsp, &mut source_fs, options.verbose)?;
      print_report(&report, selection(&options));
    },
    Command::Model { mdl, options } => {
      let source_fs = SourceFs::from_gameinfo(&options.gameinfo, options.base_dir.as_deref(), options.verbose)?;
      let report = analyze_model_file(&mdl, &source_fs)?;
      print_report(&report, selection(&options));
    },
  }
  Ok(())
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
