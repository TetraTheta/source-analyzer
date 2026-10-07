use super::material_candidates;

#[test]
fn accepts_supported_and_rejects_unknown_model_versions() {
  for version in [27, 31, 32, 35, 37, 44, 49, 52, 53, 54, 56, 58, 59] {
    assert!(material_candidates(&empty_model(version)).is_ok());
  }
  for version in [26, 33, 34, 38, 43, 50, 51, 57, 60] {
    assert!(material_candidates(&empty_model(version)).is_err());
  }
}

#[test]
fn uses_the_texture_stride_for_each_layout_family() {
  for (version, stride) in [(31, 32), (53, 44), (59, 64)] {
    let candidates = material_candidates(&model_with_two_textures(version, stride)).expect("supported synthetic MDL");
    assert_eq!(candidates, [vec!["materials/first.vmt".to_owned()], vec!["materials/second.vmt".to_owned()]]);
  }
}

fn empty_model(version: i32) -> Vec<u8> {
  let mut bytes = vec![0; 240];
  bytes[..4].copy_from_slice(b"IDST");
  bytes[4..8].copy_from_slice(&version.to_le_bytes());
  bytes[76..80].copy_from_slice(&240_i32.to_le_bytes());
  bytes
}

fn model_with_two_textures(version: i32, stride: usize) -> Vec<u8> {
  let mut bytes = vec![0; 512];
  bytes[..4].copy_from_slice(b"IDST");
  bytes[4..8].copy_from_slice(&version.to_le_bytes());
  bytes[76..80].copy_from_slice(&512_i32.to_le_bytes());
  bytes[204..208].copy_from_slice(&2_i32.to_le_bytes());
  bytes[208..212].copy_from_slice(&240_i32.to_le_bytes());

  for (index, (name, string_offset)) in [("first", 400), ("second", 420)].into_iter().enumerate() {
    let record = 240 + index * stride;
    let relative = string_offset - record;
    bytes[record..record + 4].copy_from_slice(&(relative as i32).to_le_bytes());
    bytes[string_offset..string_offset + name.len()].copy_from_slice(name.as_bytes());
  }
  bytes
}
