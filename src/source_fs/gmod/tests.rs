use super::depot_app_id;

#[test]
fn depot_names_map_to_their_steam_apps() {
  assert_eq!(depot_app_id("HL2"), Some("220"));
  assert_eq!(depot_app_id("episodic"), Some("380"));
  assert_eq!(depot_app_id("ep2"), Some("420"));
  assert_eq!(depot_app_id("unknown"), None);
}
