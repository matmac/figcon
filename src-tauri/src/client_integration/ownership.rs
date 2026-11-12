//! Schema only. No receipt IO, writer, or automatic ownership adoption.
use super::models::{Client, OwnershipStatus};
use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReceiptLifecycle {
    Prepared,
    Applying,
    Verified,
    NeedsReview,
}
/// Future private durable record under Figcon metadata. Never returned over IPC.
/// Hashes and binding identities are backend-only; no credentials are stored.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnershipRecord {
    pub schema_version: u32,
    pub receipt_id: String,
    pub client: Client,
    pub scope: String,
    pub server_name: String,
    pub configuration_identity: String,
    pub entry_sha256: String,
    pub managed_installation_identity: String,
    pub created_unix_seconds: u64,
    pub prior_entry_absent: bool,
    pub lifecycle: ReceiptLifecycle,
}
/// Even a matching deserialized record is not trusted evidence. A future loader
/// must verify provenance, current entry, config identity and transaction history.
/// Keep this distinct from executable/argument matching.
pub(super) fn assess(record: Option<&OwnershipRecord>, client: Client) -> OwnershipStatus {
    match record {
        None => OwnershipStatus::Absent,
        Some(r)
            if r.schema_version != 1
                || r.client != client
                || r.scope != "user"
                || r.server_name != super::SERVER_NAME =>
        {
            OwnershipStatus::Mismatched
        }
        Some(_) => OwnershipStatus::Unverified,
    }
}
