use super::{MapAssets, model_path, parse_entities, quoted_tokens};

#[test]
fn entity_tokenizer_keeps_key_value_order() {
  let tokens = quoted_tokens(r#"{ "classname" "prop_dynamic" "model" "models\props\a.mdl" }"#);
  assert_eq!(tokens, ["classname", "prop_dynamic", "model", r"models\props\a.mdl"]);
  assert_eq!(model_path(&tokens[3]), "models/props/a.mdl");
}

#[test]
fn numeric_material_properties_are_not_asset_paths() {
  let mut assets = MapAssets::default();

  parse_entities(br#"{ "material" "2" "texture" "decals/poster" }"#, &mut assets).expect("entities should parse");

  assert_eq!(assets.materials, ["materials/decals/poster.vmt".to_owned()].into());
}
