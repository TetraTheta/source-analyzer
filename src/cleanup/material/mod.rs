use anyhow::{Context, Result};
use kva::{KvData, KvEntry, text::Parser};

use crate::source_fs::normalize_resource;

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Node {
  name: String,
  value: Value,
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Value {
  Children(Vec<Node>),
  Text(String),
}

pub(super) fn equivalent(left: &[u8], right: &[u8]) -> Result<bool> {
  if contains_condition(left) || contains_condition(right) {
    return Ok(left == right);
  }
  let left = canonical(left).context("failed to parse target VMT")?;
  let right = canonical(right).context("failed to parse fallback VMT")?;
  Ok(left == right)
}

pub(super) fn dependencies(bytes: &[u8]) -> Result<Vec<String>> {
  let text = std::str::from_utf8(bytes).context("VMT is not UTF-8")?;
  let root = Parser::new(text).numeric_inference(false).parse().context("failed to parse VMT as Valve KeyValues1")?;
  let mut output = Vec::new();
  collect_dependencies(&root, &mut output);
  output.sort();
  output.dedup();
  Ok(output)
}

fn canonical(bytes: &[u8]) -> Result<Node> {
  let text = std::str::from_utf8(bytes).context("VMT is not UTF-8")?;
  let root = Parser::new(text).numeric_inference(false).parse().context("failed to parse VMT as Valve KeyValues1")?;
  Ok(canonical_entry(&root, false))
}

fn canonical_entry(entry: &KvEntry<'_>, ordered: bool) -> Node {
  let name = entry.name.to_ascii_lowercase();
  let value = match &entry.data {
    KvData::Compound(entries) => {
      let ordered = ordered || name == "proxies";
      let mut children = entries.iter().map(|entry| canonical_entry(entry, ordered)).collect::<Vec<_>>();
      if !ordered {
        children.sort_by(|left, right| left.name.cmp(&right.name));
      }
      Value::Children(children)
    },
    KvData::String(value) => Value::Text(canonical_text(&name, value, ordered)),
    value => Value::Text(format!("{value:?}")),
  };
  Node { name, value }
}

fn canonical_text(name: &str, value: &str, ordered: bool) -> String {
  let value = value.trim();
  if is_material_key(name) || is_texture_key(name) {
    return normalize_resource(value);
  }
  if !ordered
    && name.starts_with('$')
    && let Ok(number) = value.parse::<f64>()
    && number.is_finite()
  {
    return number.to_string();
  }
  value.to_owned()
}

fn collect_dependencies(entry: &KvEntry<'_>, output: &mut Vec<String>) {
  let name = entry.name.to_ascii_lowercase();
  match &entry.data {
    KvData::Compound(entries) => {
      for entry in entries {
        collect_dependencies(entry, output);
      }
    },
    KvData::String(value) if is_material_key(&name) => {
      let path = normalize_resource(value.trim());
      if !path.is_empty() {
        let path = path.strip_prefix("materials/").unwrap_or(&path);
        let path = path.strip_suffix(".vmt").unwrap_or(path);
        output.push(format!("materials/{path}.vmt"));
      }
    },
    KvData::String(value) if is_texture_key(&name) => {
      let path = normalize_resource(value.trim());
      if !path.is_empty() && !path.starts_with('_') && path != "env_cubemap" {
        let path = path.strip_prefix("materials/").unwrap_or(&path);
        let path = path.strip_suffix(".vtf").unwrap_or(path);
        output.push(format!("materials/{path}.vtf"));
      }
    },
    _ => {},
  }
}

fn contains_condition(bytes: &[u8]) -> bool {
  bytes.windows(2).any(|window| window == b"[$")
}

fn is_material_key(name: &str) -> bool {
  matches!(name, "include" | "$fallbackmaterial" | "$bottommaterial" | "$underwateroverlay")
}

fn is_texture_key(name: &str) -> bool {
  matches!(
    name,
    "$basetexture"
      | "$basetexture2"
      | "$blendmodulatetexture"
      | "$bumpmap"
      | "$bumpmap2"
      | "$detail"
      | "$dudvmap"
      | "$envmap"
      | "$envmapmask"
      | "$flowmap"
      | "$iris"
      | "$lightwarptexture"
      | "$normalmap"
      | "$phongexponenttexture"
      | "$selfillummask"
      | "%tooltexture"
  )
}

#[cfg(test)]
mod tests;
