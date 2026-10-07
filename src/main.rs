mod analysis;
mod bsp;
mod cleanup;
mod cli;
mod configuration;
mod model;
mod output;
mod source_fs;

use anyhow::{Context, Result, bail};
use clap::Parser;

use analysis::{Selection, analyze_map, analyze_model_file, print_report};
use cli::{AnalysisOptions, Cli, Command};
use source_fs::SourceFs;

fn main() {
  if let Err(error) = run() {
    output::error(format_args!("{error:#}"));
    std::process::exit(1);
  }
}

fn run() -> Result<()> {
  match Cli::parse().command {
    Command::Map { bsp, mut options } => {
      configuration::resolve_analysis(&mut options)?;
      let gameinfo = options.source.gameinfo.as_deref().context("--gameinfo is required unless the selected profile provides it")?;
      let mut source_fs =
        SourceFs::from_gameinfo(gameinfo, options.source.base_dir.as_deref(), &options.source.extra_base_dir, options.source.verbose)?;
      let report = analyze_map(&bsp, &mut source_fs, options.source.verbose)?;
      print_report(&report, selection(&options));
    },
    Command::Model { mdl, mut options } => {
      configuration::resolve_analysis(&mut options)?;
      let gameinfo = options.source.gameinfo.as_deref().context("--gameinfo is required unless the selected profile provides it")?;
      let source_fs = SourceFs::from_gameinfo(gameinfo, options.source.base_dir.as_deref(), &options.source.extra_base_dir, options.source.verbose)?;
      let report = analyze_model_file(&mdl, &source_fs)?;
      print_report(&report, selection(&options));
    },
    Command::Clean { mut target, mut options } => {
      configuration::resolve_cleanup(&mut target, &mut options)?;
      let target = target.as_deref().context("TARGET_DIR is required unless the selected profile provides target")?;
      let gameinfo = options.source.gameinfo.as_deref().context("--gameinfo is required unless the selected profile provides it")?;
      if options.no_backup && !options.delete {
        bail!("--no-backup requires --delete or delete = true in the selected profile");
      }
      let mut source_fs = SourceFs::from_gameinfo_excluding(
        gameinfo,
        options.source.base_dir.as_deref(),
        &options.source.extra_base_dir,
        Some(target),
        options.source.verbose,
      )?;
      cleanup::run(target, &mut source_fs, options.unused, options.delete, options.no_backup, &options.keep, options.source.verbose)?;
    },
  }
  Ok(())
}

fn selection(options: &AnalysisOptions) -> Selection {
  if options.missing {
    Selection::Missing
  } else if options.present {
    Selection::Present
  } else {
    Selection::All
  }
}
