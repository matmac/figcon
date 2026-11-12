use serde::{Deserialize, Serialize};
macro_rules! wire_enum {
    ($name:ident { $($v:ident),* $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "kebab-case")]
        pub enum $name { $($v),* }
    };
}
wire_enum!(Client {
    ClaudeCode,
    CodexCli
});
impl Client {
    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::CodexCli => "OpenAI Codex CLI",
        }
    }
}
wire_enum!(DetectionStatus {
    Missing,
    Detected,
    Ambiguous,
    UnsupportedPlatform
});
wire_enum!(ConfigurationStatus {
    Missing,
    Readable,
    Malformed,
    Inaccessible,
    Unsafe,
    UnsupportedLocation,
    UnsupportedPlatform
});
wire_enum!(RegistrationStatus {
    NotRegistered,
    MatchingExternal,
    External,
    Conflict,
    Unsupported,
    Unknown
});
wire_enum!(OwnershipStatus {
    Absent,
    Mismatched,
    Unverified
});
wire_enum!(Warning {
    RegistrationConflict,
    ReadOnly,
    ClientMissing,
    ClientAmbiguous,
    UnsupportedPlatform,
    UnsupportedLocation,
    MalformedConfiguration,
    InaccessibleConfiguration,
    UnsafeConfiguration,
    UnsupportedRegistration,
    AdditionalRegistration,
    PossibleShadowing,
    EffectiveConfigurationUnknown,
    OwnershipUnverified,
    ManagedInstallationUnavailable
});
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub client: Client,
    pub client_name: String,
    pub detection: DetectionStatus,
    pub configuration: ConfigurationStatus,
    /// A static default label, never an arbitrary environment/config value.
    pub configuration_location: String,
    pub registration: RegistrationStatus,
    pub ownership: OwnershipStatus,
    pub warnings: Vec<Warning>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationPreview {
    pub inspection: Inspection,
    pub proposed_server_name: String,
    pub proposed_executable: String,
    pub proposed_arguments: Vec<String>,
    pub scope: String,
    pub existing_registration_would_be_affected: bool,
    pub approval_required: bool,
    /// Always false: this preview is not an approval token or mutation plan.
    pub safe_to_apply: bool,
    pub managed_installation_valid: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectionError {
    pub code: String,
    pub message: String,
}
impl InspectionError {
    pub fn unavailable() -> Self {
        Self {
            code: "inspection-unavailable".into(),
            message: "Client configuration could not be inspected.".into(),
        }
    }
}
