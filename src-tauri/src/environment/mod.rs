pub mod models;
pub mod platform;

#[cfg(target_os = "macos")]
mod macos;

use models::EnvironmentInfo;
use platform::{detect_architecture, detect_operating_system};

#[tauri::command]
pub fn detect_environment(app: tauri::AppHandle) -> EnvironmentInfo {
    if crate::acceptance::legacy_blocked(&app) {
        return EnvironmentInfo {
            operating_system: detect_operating_system(),
            architecture: detect_architecture(),
            clients: Vec::new(),
        };
    }
    let mut clients = Vec::new();

    #[cfg(target_os = "macos")]
    clients.push(macos::detect_claude_desktop());

    #[cfg(target_os = "macos")]
    clients.push(macos::detect_claude_code());

    #[cfg(target_os = "macos")]
    clients.push(macos::detect_codex());

    #[cfg(target_os = "macos")]
    clients.push(macos::detect_cursor());
    #[cfg(target_os = "macos")]
    clients.push(macos::detect_gemini_cli());
    #[cfg(target_os = "macos")]
    clients.push(macos::detect_antigravity());

    EnvironmentInfo {
        operating_system: detect_operating_system(),
        architecture: detect_architecture(),
        clients,
    }
}
