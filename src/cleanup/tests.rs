use std::fs;

use super::{backup_path, discovery, model_references, used_files};
use crate::source_fs::SourceFs;

#[test]
fn map_discovery_is_limited_to_the_maps_directory_root() {
  let temp = std::env::temp_dir().join(format!("source-analyzer-cleanup-{}", std::process::id()));
  fs::create_dir_all(temp.join("maps/nested")).expect("test directories should be created");
  fs::write(temp.join("maps/root.bsp"), []).expect("root map should be written");
  fs::write(temp.join("maps/nested/ignored.bsp"), []).expect("nested map should be written");

  assert_eq!(discovery::maps(&temp).expect("maps should be listed"), [temp.join("maps/root.bsp")]);
  fs::remove_dir_all(temp).expect("temporary directory should be removed");
}

#[test]
fn model_references_and_backup_roots_are_normalized() {
  assert_eq!(model_references(b"noise models\\gibs\\part.mdl\0"), ["models/gibs/part.mdl"]);
  assert_eq!(backup_path(std::path::Path::new("target"), "materials/a/b.vmt"), std::path::Path::new("target/.materials/a/b.vmt"));
}

#[test]
fn keep_patterns_and_model_families_are_case_insensitive() {
  assert!(discovery::kept("materials/a.vmt", &["MATERIALS/*.VMT".to_owned()]));
  assert_eq!(discovery::model_group("models/Prop.DX90.VTX").as_deref(), Some("models/prop"));
}

#[test]
fn unused_scan_skips_maps_that_cannot_be_parsed() {
  let temp = std::env::temp_dir().join(format!("source-analyzer-invalid-map-{}", std::process::id()));
  let game = temp.join("game");
  let target = temp.join("addon");
  fs::create_dir_all(target.join("maps")).expect("test directories should be created");
  fs::create_dir_all(&game).expect("game directory should be created");
  fs::write(game.join("gameinfo.txt"), r#""GameInfo" { "FileSystem" { "SteamAppId" "0" "SearchPaths" { "game" "missing" } } }"#)
    .expect("gameinfo should be written");
  fs::write(target.join("maps/broken.bsp"), b"not a BSP").expect("broken map should be written");

  let mut valid_bsp = vec![0; 8 + 64 * 16 + 4];
  valid_bsp[..4].copy_from_slice(b"VBSP");
  valid_bsp[4..8].copy_from_slice(&20_i32.to_le_bytes());
  fs::write(target.join("maps/valid.bsp"), valid_bsp).expect("valid map should be written");

  let mut source_fs = SourceFs::from_gameinfo(&game.join("gameinfo.txt"), None, &[], false).expect("source filesystem should be created");
  assert!(used_files(&target, &mut source_fs, &[], false).expect("invalid map should be skipped").is_empty());

  fs::remove_dir_all(temp).expect("temporary directory should be removed");
}
