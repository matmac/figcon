use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum InstallationState {
    NotInstalled,
    Installing,
    Installed,
    UpdateAvailable,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationStatus {
    pub state: InstallationState,
    pub installed_version: Option<String>,
    pub target_version: String,
    pub message: Option<String>,
}
