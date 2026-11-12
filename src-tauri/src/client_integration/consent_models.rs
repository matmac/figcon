use super::{Client, RegistrationStatus};
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConsentCode {
    UnsupportedPlatform,
    PrerequisitesMissing,
    ClientUnavailable,
    UnsupportedConfiguration,
    Conflict,
    StalePlan,
    ExpiredPlan,
    UnknownPlan,
    ConsumedPlan,
    NotOwner,
    RegistryCapacity,
    RecoveryCapacity,
    ActivationDisabled,
    Rejected,
    OperationFailed,
    Uncertain,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsentError {
    pub code: ConsentCode,
    pub message: String,
}
impl ConsentError {
    pub(super) fn new(code: ConsentCode) -> Self {
        Self{code,message:match code {
        ConsentCode::ActivationDisabled=>"Production client configuration changes are disabled pending activation review.",
        ConsentCode::RegistryCapacity=>"Too many configuration plans are pending. Wait for plans to expire.",
        ConsentCode::RecoveryCapacity=>"Retained recovery files require review before another transaction can begin.",
        ConsentCode::PrerequisitesMissing=>"Install and verify Figcon's managed runtime and Figma Console first.",
        ConsentCode::ClientUnavailable=>"The client executable is missing or ambiguous.",
        ConsentCode::Conflict=>"An existing registration requires review. It will not be replaced.",
        ConsentCode::StalePlan=>"Configuration or prerequisites changed. Prepare a new preview.",
        ConsentCode::ExpiredPlan=>"The configuration plan expired. Prepare a new preview.",
        ConsentCode::UnknownPlan=>"This plan is unknown or has expired.",
        ConsentCode::ConsumedPlan=>"This plan has already been used.",
        ConsentCode::NotOwner=>"Use the window that prepared this plan.",
        ConsentCode::Rejected=>"This plan was declined.",
        ConsentCode::UnsupportedPlatform=>"Client configuration preparation currently requires macOS.",
        ConsentCode::UnsupportedConfiguration=>"The user configuration cannot be safely prepared.",
        ConsentCode::Uncertain=>"The transaction may have been applied. Recovery files were retained for review.",
        ConsentCode::OperationFailed=>"The configuration transaction could not be completed.",
    }.into()}
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecoveryState {
    Available,
    Full,
    Unknown,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryUsage {
    pub state: RecoveryState,
    pub artifact_count: Option<u64>,
    pub storage_bytes: Option<u64>,
    pub file_count: Option<u64>,
    pub available_disk_bytes: Option<u64>,
    pub max_artifacts: u64,
    pub max_storage_bytes: u64,
    pub max_files: u64,
    pub reserved_transaction_bytes: u64,
    pub automatic_cleanup_enabled: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsentPreview {
    pub plan_id: String,
    pub client: Client,
    pub client_name: String,
    pub proposed_server_name: String,
    pub proposed_executable: String,
    pub proposed_arguments: Vec<String>,
    pub scope: String,
    pub operation: String,
    pub registration: RegistrationStatus,
    pub summary: String,
    pub warnings: Vec<String>,
    pub expires_at_unix_ms: u64,
    pub expires_in_seconds: u64,
    pub production_mutation_enabled: bool,
    pub recovery: RecoveryUsage,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalReceipt {
    pub plan_id: String,
    pub state: String,
}
