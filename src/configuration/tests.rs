use std::path::{Path, PathBuf};

use crate::cli::SourceOptions;

use super::{apply_source, resolve_path};

#[test]
fn profile_paths_are_relative_to_the_configuration_file() {
  let directory = Path::new("config");
  assert_eq!(resolve_path(directory, "games/hl2/gameinfo.txt"), PathBuf::from("config/games/hl2/gameinfo.txt"));
}

#[test]
fn command_line_source_values_override_profile_values() {
  let profile = "gameinfo = 'profile/gameinfo.txt'\nbase_dir = 'profile/base'\nextra_base_dirs = ['profile/extra']\nverbose = true"
    .parse::<toml::Table>()
    .expect("profile should parse");
  let mut options = SourceOptions {
    profile: None,
    gameinfo: Some(PathBuf::from("command/gameinfo.txt")),
    base_dir: None,
    extra_base_dir: Vec::new(),
    verbose: false,
  };

  apply_source(&profile, Path::new("config"), &mut options).expect("profile should apply");

  assert_eq!(options.gameinfo, Some(PathBuf::from("command/gameinfo.txt")));
  assert_eq!(options.base_dir, Some(PathBuf::from("config/profile/base")));
  assert_eq!(options.extra_base_dir, [PathBuf::from("config/profile/extra")]);
  assert!(options.verbose);
}
