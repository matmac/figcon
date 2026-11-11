//! Public IPC contract. No paths, raw subprocess output, or internal errors.
use serde::{Deserialize, Serialize};

macro_rules! wire_enum {
    ($name:ident { $($variant:ident),* $(,)? }) => {
        #[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
        #[serde(rename_all = "kebab-case")]
        pub enum $name { $($variant),* }
    };
}
wire_enum!(Component { Node, Mcp });
impl Component {
    pub fn version(self) -> &'static str {
        match self {
            Self::Node => super::manifest::NODE_VERSION,
            Self::Mcp => super::manifest::MCP_VERSION,
        }
    }
    pub(super) fn index(self) -> usize {
        match self {
            Self::Node => 0,
            Self::Mcp => 1,
        }
    }
}
wire_enum!(DiskState {
    NotInstalled,
    Installed,
    Invalid
});
wire_enum!(OperationState {
    NotInstalled,
    Installing,
    Installed,
    Failed,
    Cancelled,
    Blocked
});
wire_enum!(Stage {
    Preparing,
    Downloading,
    Verifying,
    Extracting,
    Installing,
    Validating,
    Activating,
    Completed,
    Failed,
    Cancelled
});
wire_enum!(EventStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled
});
wire_enum!(ErrorCode {
    Busy,
    AlreadyInstalled,
    RuntimeRequired,
    AttemptLimit,
    UnsupportedPlatform,
    Cancelled,
    DeadlineExceeded,
    IntegrityFailed,
    ResourceLimit,
    ValidationFailed,
    OperationFailed,
    NotActive,
    NotOwner,
    StateUnavailable,
    UnstableSnapshot
});
wire_enum!(RetentionState {
    None,
    RequiresReview,
    Unknown
});
wire_enum!(CancellationDisposition {
    Requested,
    AlreadyRequested,
    TooLate
});

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstallationError {
    pub code: ErrorCode,
    pub message: String,
}
impl InstallationError {
    pub(super) fn new(code: ErrorCode) -> Self {
        let message = match code {
            ErrorCode::Busy => {
                "An installation is already active. Wait for its terminal status before retrying."
            }
            ErrorCode::AlreadyInstalled => {
                "A managed destination already exists and will not be replaced."
            }
            ErrorCode::RuntimeRequired => {
                "Install and validate the managed Node.js runtime before installing MCP."
            }
            ErrorCode::AttemptLimit => {
                "Eight retained attempts block installation. Manual staging review is required."
            }
            ErrorCode::UnsupportedPlatform => {
                "Managed installation currently requires macOS arm64 or x64."
            }
            ErrorCode::Cancelled => {
                "Installation was cancelled. Any allocated staging is retained for review."
            }
            ErrorCode::DeadlineExceeded => {
                "The installation deadline or subprocess timeout was exceeded."
            }
            ErrorCode::IntegrityFailed => "Pinned package or archive integrity validation failed.",
            ErrorCode::ResourceLimit => "An installation resource safeguard was exceeded.",
            ErrorCode::ValidationFailed => "Managed installation validation failed.",
            ErrorCode::OperationFailed => {
                "Installation failed. Inspect managed installation status before retrying."
            }
            ErrorCode::NotActive => "This attempt is not active.",
            ErrorCode::NotOwner => "This window does not own the active attempt.",
            ErrorCode::StateUnavailable => "Managed installation state could not be inspected.",
            ErrorCode::UnstableSnapshot => {
                "Installation changed during inspection. Refresh installation state."
            }
        };
        Self {
            code,
            message: message.into(),
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstallationEvent {
    pub revision: u64,
    pub schema_version: u8,
    pub attempt_id: String,
    pub component: Component,
    pub sequence: u64,
    pub stage: Stage,
    pub status: EventStatus,
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub retained_staging: bool,
    pub activated: bool,
    pub error: Option<InstallationError>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationReport {
    pub cancellation_requested: bool,
    pub attempt_id: String,
    pub component: Component,
    pub state: OperationState,
    pub target_version: String,
    pub retained_staging: bool,
    pub activated: bool,
    pub error: Option<InstallationError>,
    pub last_event: Option<InstallationEvent>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartReceipt {
    pub attempt_id: String,
    pub component: Component,
    pub state: OperationState,
    pub target_version: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CancellationReceipt {
    pub attempt_id: String,
    pub disposition: CancellationDisposition,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RetentionReport {
    pub state: RetentionState,
    pub attempt_count: Option<u32>,
    pub orphaned_ledger_files: Option<u32>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ComponentReport {
    pub disk_state: DiskState,
    pub component: Component,
    pub state: OperationState,
    pub target_version: String,
    pub installed_version: Option<String>,
    pub last_operation: Option<OperationReport>,
    pub error: Option<InstallationError>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedInstallationReport {
    pub revision: u64,
    pub supported: bool,
    pub node: ComponentReport,
    pub mcp: ComponentReport,
    pub retention: RetentionReport,
}
