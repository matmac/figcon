//! Claude user-scope JSON only; never `claude mcp get/list` (they can probe servers).
use serde::{
    Deserialize, Deserializer,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;
use std::fmt;
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(UniqueVisitor)
    }
}
struct UniqueVisitor;
impl<'de> Visitor<'de> for UniqueVisitor {
    type Value = Unique;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("JSON without duplicate keys")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Unique, M::Error> {
        let mut out = serde_json::Map::new();
        while let Some((key, value)) = map.next_entry::<String, Unique>()? {
            if out.contains_key(&key) {
                return Err(de::Error::custom("Duplicate key"));
            }
            out.insert(key, value.0);
        }
        Ok(Unique(Value::Object(out)))
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Unique, S::Error> {
        let mut out = Vec::new();
        while let Some(v) = seq.next_element::<Unique>()? {
            out.push(v.0)
        }
        Ok(Unique(Value::Array(out)))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Unique, E> {
        Ok(Unique(v.into()))
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Unique, E> {
        Ok(Unique(v.into()))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Unique, E> {
        Ok(Unique(v.into()))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Unique, E> {
        Ok(Unique(v.into()))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Unique, E> {
        Ok(Unique(
            serde_json::Number::from_f64(v)
                .ok_or_else(|| E::custom("Invalid number"))?
                .into(),
        ))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
        Ok(Unique(Value::Null))
    }
}
pub(super) fn parse(text: &str) -> Result<Value, ()> {
    let root = serde_json::from_str::<Unique>(text).map_err(|_| ())?.0;
    if !root.is_object() {
        return Err(());
    }
    Ok(root)
}
pub(super) fn shadowed(root: &Value) -> bool {
    root.get("projects")
        .and_then(Value::as_object)
        .is_some_and(|projects| {
            projects.values().any(|p| {
                p.get("mcpServers")
                    .and_then(Value::as_object)
                    .is_some_and(|s| {
                        s.keys()
                            .any(|k| k == super::SERVER_NAME || k == "figma-console")
                    })
            })
        })
}

pub(super) struct ClaudeCodeAdapter;
impl ClaudeCodeAdapter {
    pub(super) const CONFIGURATION: &'static str = ".claude.json";
    pub(super) fn inspect(text: &str) -> Result<Value, ()> {
        parse(text)
    }
}
