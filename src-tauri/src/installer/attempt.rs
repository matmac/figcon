use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum InstallationKind {
    Node,
    Mcp,
    Probe,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Lifecycle {
    Created,
    Running,
    Prepared,
    Activating,
    Activated,
    Cleaning,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub schema: u32,
    pub attempt_id: String,
    pub directory: String,
    pub kind: InstallationKind,
    pub created_unix_ms: u64,
    pub process: super::attempt_process::ProcessIdentity,
    pub state: Lifecycle,
    pub root: super::attempt_inventory::Identity,
    pub staging: super::attempt_inventory::Identity,
    pub directory_identity: super::attempt_inventory::Identity,
    pub lease_identity: super::attempt_inventory::Identity,
    pub seal: String,
}

#[cfg(target_os = "macos")]
#[path = "attempt_macos.rs"]
mod macos;
#[cfg(target_os = "macos")]
pub(super) use macos::{Journal, load, nonce as new_id, open_lease, validate_private};
#[cfg(not(target_os = "macos"))]
pub(super) fn new_id() -> Result<String, String> {
    Err("Managed installation requires macOS".into())
}
