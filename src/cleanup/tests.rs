use std::fs;

use super::{backup_path, discovery, model_references};

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
