// Figcon MCP installer.
//
// Manages the application-owned Node.js runtime
// and pinned figma-console-mcp installation.

pub mod discovery;
pub mod models;
pub mod paths;
pub mod status;

use paths::InstallerPaths;
use tauri::Manager;

pub fn resolve_installer_paths(app: &tauri::AppHandle) -> Result<InstallerPaths, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Cannot resolve application data directory: {error}"))?;

    Ok(InstallerPaths::new(root))
}
