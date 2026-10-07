use clap::Parser;

use super::{Cli, Command};

#[test]
fn clean_accepts_profile_only_and_repeatable_keep_patterns() {
  let cli = Cli::try_parse_from(["source-analyzer", "clean", "--profile", "portable", "--keep", "materials/ui/*", "--keep", "models/player/*"])
    .expect("cleanup arguments should parse");

  let Command::Clean { target, options } = cli.command else { panic!("clean command should be selected") };
  assert!(target.is_none());
  assert_eq!(options.source.profile.as_deref(), Some("portable"));
  assert_eq!(options.keep, ["materials/ui/*", "models/player/*"]);
}

#[test]
fn unused_help_explains_runtime_reference_limit() {
  let error = Cli::try_parse_from(["source-analyzer", "clean", "--help"]).expect_err("help exits through clap");
  let help = error.to_string();
  assert!(help.contains("Runtime code references are not analyzed"));
  assert!(help.contains("skip their dependency inspection"));
}
