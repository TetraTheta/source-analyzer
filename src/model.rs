use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};

use crate::source_fs::normalize_resource;

const MIN_HEADER_SIZE: usize = 240;
const TEXTURE_COUNT_OFFSET: usize = 204;
const TEXTURE_TABLE_OFFSET: usize = 208;
const TEXTURE_DIRECTORY_COUNT_OFFSET: usize = 212;
const TEXTURE_DIRECTORY_TABLE_OFFSET: usize = 216;
const SKIN_REFERENCE_COUNT_OFFSET: usize = 220;
const SKIN_FAMILY_COUNT_OFFSET: usize = 224;
const SKIN_TABLE_OFFSET: usize = 228;
const BODY_PART_COUNT_OFFSET: usize = 232;
const BODY_PART_TABLE_OFFSET: usize = 236;
const BODY_PART_RECORD_SIZE: usize = 16;
const MODEL_MESH_COUNT_OFFSET: usize = 72;
const MODEL_MESH_TABLE_OFFSET: usize = 76;

#[derive(Clone, Copy)]
struct MdlLayout {
  mesh_record_size: usize,
  model_record_size: usize,
  texture_record_size: usize,
}

pub fn material_candidates(bytes: &[u8]) -> Result<Vec<Vec<String>>> {
  let layout = validate_header(bytes)?;
  let textures = read_textures(bytes, layout)?;
  let directories = read_directories(bytes)?;
  let directories = if directories.is_empty() { vec![String::new()] } else { directories };
  let mesh_slots = read_mesh_slots(bytes, layout)?;
  let texture_indices = read_used_texture_indices(bytes, textures.len(), &mesh_slots)?;

  let mut output = Vec::new();
  for index in texture_indices {
    let Some(texture) = textures.get(index) else {
      continue;
    };
    let normalized_name = normalize_resource(texture);
    let name = normalized_name.strip_suffix(".vmt").or_else(|| normalized_name.strip_suffix(".vtf")).unwrap_or(&normalized_name);
    let candidates = directories
      .iter()
      .map(|directory| {
        let directory = directory.trim_matches(['/', '\\']);
        let path = if directory.is_empty() { format!("materials/{name}.vmt") } else { format!("materials/{directory}/{name}.vmt") };
        normalize_resource(&path)
      })
      .collect::<BTreeSet<_>>()
      .into_iter()
      .collect();
    output.push(candidates);
  }
  Ok(output)
}

fn validate_header(bytes: &[u8]) -> Result<MdlLayout> {
  if bytes.len() < MIN_HEADER_SIZE {
    bail!("MDL is smaller than its required header");
  }
  if &bytes[..4] != b"IDST" {
    bail!("invalid MDL signature");
  }
  let version = read_i32(bytes, 4)?;
  let layout = layout_for_version(version)
    .with_context(|| format!("unsupported MDL version {version}; supported versions are 27-32, 35-37, 44-49, 52-56, and 58-59"))?;
  let declared_length = read_i32(bytes, 76)?;
  if declared_length < MIN_HEADER_SIZE as i32 || declared_length as usize > bytes.len() {
    bail!("invalid MDL declared length {declared_length}");
  }
  Ok(layout)
}

fn layout_for_version(version: i32) -> Option<MdlLayout> {
  let layout = match version {
    27 => MdlLayout { mesh_record_size: 96, model_record_size: 120, texture_record_size: 32 },
    28..=30 => MdlLayout { mesh_record_size: 68, model_record_size: 120, texture_record_size: 32 },
    31 => MdlLayout { mesh_record_size: 68, model_record_size: 108, texture_record_size: 32 },
    32 | 35..=37 => MdlLayout { mesh_record_size: 68, model_record_size: 140, texture_record_size: 32 },
    44..=49 | 52 | 54..=56 | 58..=59 => MdlLayout { mesh_record_size: 116, model_record_size: 148, texture_record_size: 64 },
    53 => MdlLayout { mesh_record_size: 116, model_record_size: 148, texture_record_size: 44 },
    _ => return None,
  };
  Some(layout)
}

fn read_textures(bytes: &[u8], layout: MdlLayout) -> Result<Vec<String>> {
  let count = read_count(bytes, TEXTURE_COUNT_OFFSET, "texture count")?;
  let table = read_offset(bytes, TEXTURE_TABLE_OFFSET, "texture table")?;
  checked_range(bytes, table, count, layout.texture_record_size, "texture table")?;

  (0..count)
    .map(|index| {
      let record = table + index * layout.texture_record_size;
      let relative = read_offset(bytes, record, "texture name")?;
      let name_offset = record.checked_add(relative).context("texture name offset overflow")?;
      read_c_string(bytes, name_offset, "texture name")
    })
    .collect()
}

fn read_directories(bytes: &[u8]) -> Result<Vec<String>> {
  let count = read_count(bytes, TEXTURE_DIRECTORY_COUNT_OFFSET, "texture directory count")?;
  let table = read_offset(bytes, TEXTURE_DIRECTORY_TABLE_OFFSET, "texture directory table")?;
  checked_range(bytes, table, count, 4, "texture directory table")?;

  (0..count)
    .map(|index| {
      let string_offset = read_offset(bytes, table + index * 4, "texture directory")?;
      read_c_string(bytes, string_offset, "texture directory")
    })
    .collect()
}

fn read_mesh_slots(bytes: &[u8], layout: MdlLayout) -> Result<BTreeSet<usize>> {
  let body_part_count = read_count(bytes, BODY_PART_COUNT_OFFSET, "body part count")?;
  let body_part_table = read_offset(bytes, BODY_PART_TABLE_OFFSET, "body part table")?;
  checked_range(bytes, body_part_table, body_part_count, BODY_PART_RECORD_SIZE, "body part table")?;

  let mut slots = BTreeSet::new();
  for part_index in 0..body_part_count {
    let part = body_part_table + part_index * BODY_PART_RECORD_SIZE;
    let model_count = read_count(bytes, part + 4, "model count")?;
    let model_relative = read_offset(bytes, part + 12, "model table")?;
    let model_table = part.checked_add(model_relative).context("model table offset overflow")?;
    checked_range(bytes, model_table, model_count, layout.model_record_size, "model table")?;

    for model_index in 0..model_count {
      let model = model_table + model_index * layout.model_record_size;
      let mesh_count = read_count(bytes, model + MODEL_MESH_COUNT_OFFSET, "mesh count")?;
      let mesh_relative = read_offset(bytes, model + MODEL_MESH_TABLE_OFFSET, "mesh table")?;
      let mesh_table = model.checked_add(mesh_relative).context("mesh table offset overflow")?;
      checked_range(bytes, mesh_table, mesh_count, layout.mesh_record_size, "mesh table")?;
      for mesh_index in 0..mesh_count {
        let mesh = mesh_table + mesh_index * layout.mesh_record_size;
        if let Ok(slot) = usize::try_from(read_i32(bytes, mesh)?) {
          slots.insert(slot);
        }
      }
    }
  }
  Ok(slots)
}

fn read_used_texture_indices(bytes: &[u8], texture_count: usize, mesh_slots: &BTreeSet<usize>) -> Result<BTreeSet<usize>> {
  let references = read_count(bytes, SKIN_REFERENCE_COUNT_OFFSET, "skin reference count")?;
  let families = read_count(bytes, SKIN_FAMILY_COUNT_OFFSET, "skin family count")?;
  if references == 0 || families == 0 {
    let indices = mesh_slots.iter().copied().filter(|index| *index < texture_count).collect::<BTreeSet<_>>();
    return Ok(if indices.is_empty() { (0..texture_count).collect() } else { indices });
  }

  let table = read_offset(bytes, SKIN_TABLE_OFFSET, "skin table")?;
  let entries = references.checked_mul(families).context("skin table entry count overflow")?;
  let range = checked_range(bytes, table, entries, 2, "skin table")?;
  let mut indices = BTreeSet::new();
  for family in range.chunks_exact(references * 2) {
    for slot in mesh_slots {
      if let Some(entry) = family.get(*slot * 2..*slot * 2 + 2) {
        let index = u16::from_le_bytes([entry[0], entry[1]]) as usize;
        if index < texture_count {
          indices.insert(index);
        }
      }
    }
  }
  if indices.is_empty() {
    indices.extend(0..texture_count);
  }
  Ok(indices)
}

fn checked_range<'a>(bytes: &'a [u8], offset: usize, count: usize, item_size: usize, field: &str) -> Result<&'a [u8]> {
  let size = count.checked_mul(item_size).with_context(|| format!("{field} size overflow"))?;
  let end = offset.checked_add(size).with_context(|| format!("{field} offset overflow"))?;
  bytes.get(offset..end).with_context(|| format!("{field} is out of bounds"))
}

fn read_count(bytes: &[u8], offset: usize, field: &str) -> Result<usize> {
  usize::try_from(read_i32(bytes, offset)?).with_context(|| format!("{field} must not be negative"))
}

fn read_offset(bytes: &[u8], offset: usize, field: &str) -> Result<usize> {
  usize::try_from(read_i32(bytes, offset)?).with_context(|| format!("{field} offset must not be negative"))
}

fn read_c_string(bytes: &[u8], offset: usize, field: &str) -> Result<String> {
  let tail = bytes.get(offset..).with_context(|| format!("{field} offset is out of bounds"))?;
  let end = tail.iter().position(|byte| *byte == 0).with_context(|| format!("{field} is not null-terminated"))?;
  let value = std::str::from_utf8(&tail[..end]).with_context(|| format!("{field} is not valid UTF-8"))?;
  Ok(value.replace('\\', "/"))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32> {
  let value = bytes.get(offset..offset + 4).context("MDL field is out of bounds")?;
  Ok(i32::from_le_bytes(value.try_into().expect("four-byte slice")))
}

#[cfg(test)]
mod tests {
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
}
