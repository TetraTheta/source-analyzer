use super::{dependencies, equivalent};

#[test]
fn vmt_comparison_normalizes_keys_paths_order_and_numbers() {
  let left = br#"UnlitGeneric { "$BaseTexture" "Folder\Texture" "$Alpha" "1" }"#;
  let right = br#"unlitgeneric { "$alpha" "1.0" "$basetexture" "folder/texture" }"#;

  assert!(equivalent(left, right).expect("VMT files should parse"));
}

#[test]
fn dependencies_include_materials_and_textures() {
  let vmt = br#"Patch { "include" "Base/Foo" "insert" { "$bumpmap" "Detail/Normal" } }"#;

  assert_eq!(dependencies(vmt).expect("VMT should parse"), ["materials/base/foo.vmt", "materials/detail/normal.vtf"]);
}

#[test]
fn comparison_error_identifies_the_invalid_side() {
  let error = equivalent(br#"UnlitGeneric { "$alpha" "1" }"#, b"{").expect_err("fallback VMT should be rejected");

  assert!(format!("{error:#}").contains("failed to parse fallback VMT"));
}
