//! Codex user TOML only. Unknown top-level settings are never serialized to IPC.
use serde_json::Value;
pub(super) fn parse(text: &str) -> Result<Value, ()> {
    let root = text.parse::<toml::Table>().map_err(|_| ())?;
    serde_json::to_value(root).map_err(|_| ())
}

pub(super) struct CodexCliAdapter;
impl CodexCliAdapter {
    pub(super) const CONFIGURATION: &'static str = ".codex/config.toml";
    pub(super) fn inspect(text: &str) -> Result<Value, ()> {
        parse(text)
    }
}
