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
    if let Some(roots) = crate::acceptance::roots(app).map_err(str::to_owned)? {
        return Ok(InstallerPaths::new(roots.data.join("figcon-installer")));
    }
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Cannot resolve application data directory: {error}"))?
        .join("figcon-installer");

    Ok(InstallerPaths::new(root))
}

pub mod validate;

pub mod activate;

pub mod install;

#[cfg(target_os = "macos")]
mod admission;
#[cfg(target_os = "macos")]
mod anchored;
pub mod api;
mod api_backend;
mod api_models;
mod api_service;
mod attempt;
mod attempt_inventory;
mod attempt_process;
mod deadline;
mod limits;
pub mod mcp;
mod mcp_contract;
mod mcp_fs;
mod npm;
mod process;
mod progress;
pub mod recovery;
mod runtime_aliases;

#[cfg(all(test, target_os = "macos"))]
mod recovery_tests;

#[cfg(all(test, unix))]
mod test_support;

#[cfg(all(test, unix))]
mod node_archive_tests;

#[cfg(all(test, target_os = "macos", target_arch = "aarch64"))]
mod e2e_tests;

#[cfg(test)]
mod api_tests;

#[cfg(all(test, target_os = "macos"))]
mod external_config_tests;
