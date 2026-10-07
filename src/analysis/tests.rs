use super::{Selection, selection_allows, status_style};
use crate::{output, source_fs::AssetStatus};

#[test]
fn present_selection_includes_embedded_assets() {
  assert!(selection_allows(Selection::Present, AssetStatus::Present));
  assert!(selection_allows(Selection::Present, AssetStatus::Embedded));
  assert!(!selection_allows(Selection::Present, AssetStatus::Missing));
}

#[test]
fn report_status_colors_follow_severity() {
  assert_eq!(status_style(AssetStatus::Missing), output::ERROR);
  assert_eq!(status_style(AssetStatus::Embedded), output::EMBEDDED);
  assert_eq!(status_style(AssetStatus::Present), output::PRESENT);
}

#[test]
fn report_titles_use_readable_title_case() {
  assert_eq!(super::title_case("MISSING MAP MATERIAL"), "Missing Map Material");
}
