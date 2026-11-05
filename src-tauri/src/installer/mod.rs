pub mod cleanup;
// Figcon MCP installer.
//
// Manages the application-owned Node.js runtime
// and pinned figma-console-mcp installation.

pub mod checksum;
pub mod discovery;
pub mod download;
pub mod extract;
pub mod manifest;
pub mod models;
pub mod paths;
pub mod root;
pub mod runtime;
pub mod staging;
pub mod status;
pub mod workspace;

use paths::InstallerPaths;
use tauri::Manager;

pub fn resolve_installer_paths(app: &tauri::AppHandle) -> Result<InstallerPaths, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Cannot resolve application data directory: {error}"))?;

    Ok(InstallerPaths::new(root))
}

pub mod validate;
