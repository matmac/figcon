//! Client-specific edits. No filesystem access or command execution.
use super::{Code, Result};
use crate::client_integration::{Client, SERVER_NAME, claude, codex, related};
use serde_json::{Value, value::RawValue};
use std::collections::BTreeMap;
pub(super) fn parse(client: Client, text: &str) -> Result<Value> {
    match client {
        Client::ClaudeCode => claude::parse(text),
        Client::CodexCli => codex::parse(text),
    }
    .map_err(|_| Code::Malformed)
}
pub(super) fn key(client: Client) -> &'static str {
    match client {
        Client::ClaudeCode => "mcpServers",
        Client::CodexCli => "mcp_servers",
    }
}
pub(super) fn entry(client: Client, executable: &str, args: &[String]) -> Value {
    let mut entry = serde_json::json!({"command":executable,"args":args});
    if client == Client::ClaudeCode {
        entry["type"] = "stdio".into();
    }
    entry
}
fn expected(client: Client, baseline: &Value, entry: &Value) -> Result<Value> {
    let mut after = baseline.clone();
    let root = after.as_object_mut().ok_or(Code::Malformed)?;
    let servers = root
        .entry(key(client))
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or(Code::Unsupported)?;
    if servers.iter().any(|(name, value)| related(name, value)) {
        return Err(Code::Conflict);
    }
    servers.insert(SERVER_NAME.into(), entry.clone());
    Ok(after)
}
/// JSON is validated by the duplicate-rejecting parser first. RawValue spans
/// identify only object boundaries; we do not implement a second JSON grammar.
fn json_add(text: &str, entry: &Value) -> Result<String> {
    let objects: BTreeMap<String, &RawValue> =
        serde_json::from_str(text).map_err(|_| Code::Malformed)?;
    let property = format!(
        "{}:{}",
        serde_json::to_string(super::super::SERVER_NAME).map_err(|_| Code::Malformed)?,
        entry
    );
    let (offset, insertion) = if let Some(raw) = objects.get("mcpServers") {
        let members: BTreeMap<String, &RawValue> =
            serde_json::from_str(raw.get()).map_err(|_| Code::Unsupported)?;
        let end = raw.get().trim_end().len() - 1;
        let offset = raw.get().as_ptr() as usize - text.as_ptr() as usize + end;
        (
            offset,
            format!("{}{}", if members.is_empty() { "" } else { "," }, property),
        )
    } else {
        (
            text.trim_end().len() - 1,
            format!(
                "{}\"mcpServers\":{{{}}}",
                if objects.is_empty() { "" } else { "," },
                property
            ),
        )
    };
    let mut candidate = text.to_owned();
    candidate.insert_str(offset, &insertion);
    Ok(candidate)
}
fn toml_add(text: &str, entry: &Value) -> Result<String> {
    use toml_edit::{Array, DocumentMut, Item, Table, value};
    let mut doc = text.parse::<DocumentMut>().map_err(|_| Code::Malformed)?;
    if !doc.contains_key("mcp_servers") {
        doc["mcp_servers"] = Item::Table(Table::new());
    }
    let servers = doc
        .get_mut("mcp_servers")
        .and_then(Item::as_table_mut)
        .ok_or(Code::Unsupported)?;
    if servers.is_dotted() {
        return Err(Code::Unsupported);
    }
    let mut table = Table::new();
    table.insert(
        "command",
        value(entry["command"].as_str().ok_or(Code::Unsupported)?),
    );
    let mut args = Array::new();
    for arg in entry["args"].as_array().ok_or(Code::Unsupported)? {
        args.push(arg.as_str().ok_or(Code::Unsupported)?);
    }
    table.insert("args", value(args));
    servers.insert(SERVER_NAME, Item::Table(table));
    Ok(doc.to_string())
}
pub(super) fn add(client: Client, text: Option<&str>, entry: &Value) -> Result<String> {
    let text = text.unwrap_or(match client {
        Client::ClaudeCode => "{}",
        Client::CodexCli => "",
    });
    let before = parse(client, text)?;
    let expected = expected(client, &before, entry)?;
    let candidate = match client {
        Client::ClaudeCode => json_add(text, entry)?,
        Client::CodexCli => toml_add(text, entry)?,
    };
    if parse(client, &candidate)? != expected {
        return Err(Code::Verification);
    }
    Ok(candidate)
}
pub(super) fn verify(
    client: Client,
    before: Option<&str>,
    after: &str,
    entry: &Value,
) -> Result<()> {
    let before = parse(
        client,
        before.unwrap_or(match client {
            Client::ClaudeCode => "{}",
            Client::CodexCli => "",
        }),
    )?;
    if parse(client, after)? != expected(client, &before, entry)? {
        return Err(Code::Verification);
    }
    Ok(())
}
