use std::fs;

use kva::text::Parser;

use super::{SourceFs, find_compound, resolve_search_paths, wildcard_matches};

#[test]
fn compound_lookup_accepts_root_and_nested_blocks() {
  for (text, name) in [
    (r#""mountcfg" { "development" "C:\\content" }"#, "mountcfg"),
    (r#""gamedepotsystem" { "hl2" "1" }"#, "gamedepotsystem"),
    (r#""libraryfolders" { "0" { "path" "C:\\Steam" } }"#, "libraryfolders"),
    (r#""GameInfo" { "FileSystem" { "SteamAppId" "4000" } }"#, "FileSystem"),
  ] {
    let root = Parser::new(text).numeric_inference(false).parse().expect("KeyValues should parse");
    assert!(!find_compound(&root, name).expect("compound should be found").is_empty());
  }
}

#[test]
fn wildcard_matching_is_case_insensitive() {
  assert!(wildcard_matches("*.VPK", "pak01_dir.vpk"));
  assert!(wildcard_matches("addon_?", "ADDON_1"));
  assert!(!wildcard_matches("*.vpk", "materials"));
}

#[test]
fn search_paths_use_primary_then_extra_base_dirs() {
  let temp = std::env::temp_dir().join(format!("source-analyzer-{}", std::process::id()));
  let primary = temp.join("primary");
  let extra = temp.join("extra");
  fs::create_dir_all(primary.join("game")).expect("primary search path should be created");
  fs::create_dir_all(extra.join("game")).expect("extra search path should be created");

  let paths = resolve_search_paths("game", &temp, &primary, std::slice::from_ref(&extra)).expect("search paths should resolve");

  assert_eq!(paths, [primary.join("game"), extra.join("game")]);
  fs::remove_dir_all(&temp).expect("temporary search paths should be removed");
}

#[test]
fn extra_base_dir_is_also_a_loose_content_root() {
  let temp = std::env::temp_dir().join(format!("source-analyzer-extra-content-{}", std::process::id()));
  let game = temp.join("game");
  let extra = temp.join("addon");
  fs::create_dir_all(extra.join("materials")).expect("extra content path should be created");
  fs::create_dir_all(&game).expect("game path should be created");
  fs::write(game.join("gameinfo.txt"), r#""GameInfo" { "FileSystem" { "SteamAppId" "0" "SearchPaths" { "game" "missing" } } }"#)
    .expect("gameinfo should be written");
  fs::write(extra.join("materials/example.vmt"), b"LightmappedGeneric {}").expect("material should be written");

  let source_fs =
    SourceFs::from_gameinfo(&game.join("gameinfo.txt"), None, std::slice::from_ref(&extra), false).expect("source filesystem should be created");

  assert!(source_fs.resolve("materials/example.vmt").is_some());
  fs::remove_dir_all(&temp).expect("temporary content should be removed");
}
