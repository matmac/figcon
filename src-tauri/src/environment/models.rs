use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OperatingSystem {
    Macos,
    Windows,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Architecture {
    Arm64,
    X64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ClientId {
    ClaudeDesktop,
    ClaudeCode,
    Codex,
    Cursor,
    GeminiCli,
    Antigravity,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum InstallationStatus {
    Installed,
    NotInstalled,
    Unknown,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DetectionMethod {
    ApplicationBundle,
    Executable,
    ConfigurationDirectory,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedClient {
    pub id: ClientId,
    pub status: InstallationStatus,
    pub detection_method: Option<DetectionMethod>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentInfo {
    pub operating_system: OperatingSystem,
    pub architecture: Architecture,
    pub clients: Vec<DetectedClient>,
}
