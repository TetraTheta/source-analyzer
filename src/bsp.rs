use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};

use crate::source_fs::normalize_resource;

const HEADER_SIZE: usize = 8 + 64 * 16 + 4;
const LUMP_ENTITIES: usize = 0;
const LUMP_TEXDATA: usize = 2;
const LUMP_GAME: usize = 35;
const LUMP_PAKFILE: usize = 40;
const LUMP_TEXDATA_STRING_DATA: usize = 43;
const LUMP_TEXDATA_STRING_TABLE: usize = 44;
const TEXDATA_SIZE: usize = 32;

#[derive(Debug, Default)]
pub struct MapAssets {
  pub materials: BTreeSet<String>,
  pub models: BTreeSet<String>,
  pub pakfile: Vec<u8>,
  pub particles: BTreeSet<String>,
  pub sounds: BTreeSet<String>,
}

#[derive(Clone, Copy)]
struct Lump {
  offset: usize,
  length: usize,
}

pub fn parse(bytes: &[u8]) -> Result<MapAssets> {
  if bytes.len() < HEADER_SIZE {
    bail!("BSP is smaller than its header");
  }
  if &bytes[..4] != b"VBSP" {
    bail!("invalid BSP signature");
  }
  let version = read_i32(bytes, 4)?;
  if !(19..=20).contains(&version) {
    bail!("unsupported BSP version {version}; expected 19 or 20");
  }
  let lumps = parse_lumps(bytes)?;
  let mut assets = MapAssets { pakfile: slice_lump(bytes, lumps[LUMP_PAKFILE])?.to_vec(), ..Default::default() };
  parse_world_materials(bytes, &lumps, &mut assets.materials)?;
  parse_game_lumps(bytes, lumps[LUMP_GAME], &mut assets)?;
  parse_entities(slice_lump(bytes, lumps[LUMP_ENTITIES])?, &mut assets)?;
  Ok(assets)
}

fn parse_lumps(bytes: &[u8]) -> Result<[Lump; 64]> {
  let mut lumps = [Lump { offset: 0, length: 0 }; 64];
  for (index, lump) in lumps.iter_mut().enumerate() {
    let base = 8 + index * 16;
    let offset = read_i32(bytes, base)?;
    let length = read_i32(bytes, base + 4)?;
    let uncompressed_length = read_i32(bytes, base + 12)?;
    if offset < 0 || length < 0 {
      bail!("BSP lump {index} has a negative range");
    }
    if uncompressed_length != 0 {
      bail!("compressed BSP lump {index} is not supported yet");
    }
    let offset = offset as usize;
    let length = length as usize;
    offset.checked_add(length).filter(|end| *end <= bytes.len()).with_context(|| format!("BSP lump {index} is out of bounds"))?;
    *lump = Lump { offset, length };
  }
  Ok(lumps)
}

fn parse_world_materials(bytes: &[u8], lumps: &[Lump; 64], materials: &mut BTreeSet<String>) -> Result<()> {
  let texdata = slice_lump(bytes, lumps[LUMP_TEXDATA])?;
  let string_data = slice_lump(bytes, lumps[LUMP_TEXDATA_STRING_DATA])?;
  let string_table = slice_lump(bytes, lumps[LUMP_TEXDATA_STRING_TABLE])?;
  if texdata.len() % TEXDATA_SIZE != 0 || string_table.len() % 4 != 0 {
    bail!("invalid BSP texture data layout");
  }
  for record in texdata.as_chunks::<TEXDATA_SIZE>().0 {
    let table_index = read_i32(record, 12)?;
    let table_offset = usize::try_from(table_index)
      .ok()
      .and_then(|index| string_table.get(index * 4..index * 4 + 4))
      .map(|bytes| i32::from_le_bytes(bytes.try_into().expect("four-byte slice")))
      .context("BSP texdata string table index is out of bounds")?;
    let name = read_c_string(string_data, table_offset)?;
    if !name.is_empty() {
      materials.insert(material_path(name));
    }
  }
  Ok(())
}

fn parse_game_lumps(bytes: &[u8], lump: Lump, assets: &mut MapAssets) -> Result<()> {
  if lump.length == 0 {
    return Ok(());
  }
  let game = slice_lump(bytes, lump)?;
  let count = usize::try_from(read_i32(game, 0)?).context("negative BSP game lump count")?;
  let directory_size = count.checked_mul(16).and_then(|size| size.checked_add(4)).context("BSP game lump directory overflow")?;
  if directory_size > game.len() {
    bail!("BSP game lump directory is out of bounds");
  }
  for index in 0..count {
    let base = 4 + index * 16;
    let id = read_u32(game, base)?;
    let version = read_u16(game, base + 6)?;
    let offset = usize::try_from(read_i32(game, base + 8)?).context("negative game lump offset")?;
    let length = usize::try_from(read_i32(game, base + 12)?).context("negative game lump length")?;
    let data = bytes.get(offset..offset.checked_add(length).context("game lump overflow")?).context("game lump data is out of bounds")?;
    if id == u32::from_be_bytes(*b"sprp") {
      parse_model_dictionary(data, &mut assets.models)?;
    } else if id == u32::from_be_bytes(*b"dprp") {
      let consumed = parse_model_dictionary(data, &mut assets.models)?;
      if version == 4 {
        let sprite_count = read_i32(data, consumed)?;
        if sprite_count > 0 {
          assets.materials.insert("materials/detail/detailsprites.vmt".to_owned());
        }
      }
    }
  }
  Ok(())
}

fn parse_model_dictionary(data: &[u8], models: &mut BTreeSet<String>) -> Result<usize> {
  let count = usize::try_from(read_i32(data, 0)?).context("negative model count")?;
  let end = count.checked_mul(128).and_then(|size| size.checked_add(4)).context("model dictionary overflow")?;
  let records = data.get(4..end).context("model dictionary is out of bounds")?;
  for record in records.as_chunks::<128>().0 {
    let name = read_c_string(record, 0)?;
    if !name.is_empty() {
      models.insert(model_path(name));
    }
  }
  Ok(end)
}

fn parse_entities(data: &[u8], assets: &mut MapAssets) -> Result<()> {
  let text = std::str::from_utf8(data.strip_suffix(&[0]).unwrap_or(data)).context("BSP entity lump is not UTF-8")?;
  let tokens = quoted_tokens(text);
  for pair in tokens.as_chunks::<2>().0 {
    let key = pair[0].as_str();
    let value = pair[1].trim();
    let lower = value.to_ascii_lowercase();
    if lower.ends_with(".mdl") {
      assets.models.insert(model_path(value));
    } else if lower.ends_with(".vmt") {
      assets.materials.insert(material_path(value));
    } else if lower.ends_with(".wav") || lower.ends_with(".mp3") {
      assets.sounds.insert(sound_path(value));
    } else if lower.ends_with(".pcf") {
      assets.particles.insert(particle_path(value));
    } else if is_model_key(key) && lower.starts_with("models/") {
      assets.models.insert(model_path(&format!("{value}.mdl")));
    } else if is_material_key(key) && !value.is_empty() {
      assets.materials.insert(material_path(value));
    }
  }
  Ok(())
}

fn quoted_tokens(text: &str) -> Vec<String> {
  let mut output = Vec::new();
  let mut chars = text.chars().peekable();
  while let Some(character) = chars.next() {
    if character != '"' {
      continue;
    }
    let mut token = String::new();
    while let Some(character) = chars.next() {
      if character == '"' {
        break;
      } else if character == '\\' && matches!(chars.peek(), Some('"' | '\\')) {
        token.push(chars.next().expect("peeked character"));
      } else {
        token.push(character);
      }
    }
    output.push(token);
  }
  output
}

fn is_model_key(key: &str) -> bool {
  matches_ignore_ascii_case(key, &["model", "gibmodel", "shootmodel"])
}

fn is_material_key(key: &str) -> bool {
  matches_ignore_ascii_case(key, &["material", "overlaymaterial", "ropematerial", "spritename", "texture"])
}

fn matches_ignore_ascii_case(value: &str, values: &[&str]) -> bool {
  values.iter().any(|candidate| value.eq_ignore_ascii_case(candidate))
}

fn material_path(value: &str) -> String {
  let value = normalize_resource(value.trim_matches(['/', '\\']));
  let value = value.strip_suffix(".vmt").unwrap_or(&value);
  let value = value.strip_prefix("materials/").unwrap_or(value);
  normalize_resource(&format!("materials/{value}.vmt"))
}

fn model_path(value: &str) -> String {
  let value = normalize_resource(value.trim_matches(['/', '\\']));
  let value = value.strip_prefix("models/").unwrap_or(&value);
  let value = value.strip_suffix(".mdl").unwrap_or(value);
  format!("models/{value}.mdl")
}

fn sound_path(value: &str) -> String {
  let value = value.trim_start_matches(['*', '#', '@', '<', '>', '^', ')', '(']);
  let value = normalize_resource(value.trim_matches(['/', '\\']));
  let value = value.strip_prefix("sound/").unwrap_or(&value);
  format!("sound/{value}")
}

fn particle_path(value: &str) -> String {
  let value = normalize_resource(value.trim_matches(['/', '\\']));
  let value = value.strip_prefix("particles/").unwrap_or(&value);
  format!("particles/{value}")
}

fn slice_lump(bytes: &[u8], lump: Lump) -> Result<&[u8]> {
  bytes.get(lump.offset..lump.offset + lump.length).context("BSP lump is out of bounds")
}

fn read_c_string(data: &[u8], offset: i32) -> Result<&str> {
  let offset = usize::try_from(offset).context("negative string offset")?;
  let data = data.get(offset..).context("string offset is out of bounds")?;
  let end = data.iter().position(|byte| *byte == 0).unwrap_or(data.len());
  std::str::from_utf8(&data[..end]).context("BSP string is not UTF-8")
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32> {
  let bytes = bytes.get(offset..offset + 4).context("BSP field is out of bounds")?;
  Ok(i32::from_le_bytes(bytes.try_into().expect("four-byte slice")))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
  let bytes = bytes.get(offset..offset + 4).context("BSP field is out of bounds")?;
  Ok(u32::from_le_bytes(bytes.try_into().expect("four-byte slice")))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
  let bytes = bytes.get(offset..offset + 2).context("BSP field is out of bounds")?;
  Ok(u16::from_le_bytes(bytes.try_into().expect("two-byte slice")))
}

#[cfg(test)]
mod tests {
  use super::{model_path, quoted_tokens};

  #[test]
  fn entity_tokenizer_keeps_key_value_order() {
    let tokens = quoted_tokens(r#"{ "classname" "prop_dynamic" "model" "models\props\a.mdl" }"#);
    assert_eq!(tokens, ["classname", "prop_dynamic", "model", r"models\props\a.mdl"]);
    assert_eq!(model_path(&tokens[3]), "models/props/a.mdl");
  }
}
