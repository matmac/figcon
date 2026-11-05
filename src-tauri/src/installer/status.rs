use super::models::{InstallationState, InstallationStatus};
use super::resolve_installer_paths;

#[tauri::command]
pub fn get_installation_status(app: tauri::AppHandle) -> Result<InstallationStatus, String> {
    let paths = resolve_installer_paths(&app)?;

    let state = if paths.mcp.exists() {
        InstallationState::Error
    } else {
        InstallationState::NotInstalled
    };

    Ok(InstallationStatus {
        state,
        installed_version: None,
        target_version: "1.46.0".to_string(),
        message: Some("MCP installation verification is not implemented yet.".to_string()),
    })
}
