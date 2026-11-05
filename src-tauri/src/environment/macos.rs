use std::path::PathBuf;

use super::models::{ClientId, DetectedClient, DetectionMethod, InstallationStatus};

fn detect_claude_desktop_at_paths(paths: &[PathBuf]) -> DetectedClient {
    let installed = paths.iter().any(|path| path.is_dir());

    DetectedClient {
        id: ClientId::ClaudeDesktop,
        status: if installed {
            InstallationStatus::Installed
        } else {
            InstallationStatus::Unknown
        },
        detection_method: if installed {
            Some(DetectionMethod::ApplicationBundle)
        } else {
            None
        },
        version: None,
    }
}

pub fn detect_claude_desktop() -> DetectedClient {
    let mut locations = vec![PathBuf::from("/Applications/Claude.app")];

    if let Some(home) = std::env::var_os("HOME") {
        locations.push(PathBuf::from(home).join("Applications/Claude.app"));
    }

    detect_claude_desktop_at_paths(&locations)
}

fn detect_claude_code_at_paths(paths: &[PathBuf]) -> DetectedClient {
    let installed = paths.iter().any(|path| path.is_file());

    DetectedClient {
        id: ClientId::ClaudeCode,
        status: if installed {
            InstallationStatus::Installed
        } else {
            InstallationStatus::Unknown
        },
        detection_method: if installed {
            Some(DetectionMethod::Executable)
        } else {
            None
        },
        version: None,
    }
}

pub fn detect_claude_code() -> DetectedClient {
    let mut locations = vec![
        PathBuf::from("/opt/homebrew/bin/claude"),
        PathBuf::from("/usr/local/bin/claude"),
    ];

    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);

        locations.push(home.join(".local/bin/claude"));
        locations.push(home.join(".npm-global/bin/claude"));
    }

    detect_claude_code_at_paths(&locations)
}

fn detect_codex_at_paths(applications: &[PathBuf], executables: &[PathBuf]) -> DetectedClient {
    let application_found = applications.iter().any(|path| path.is_dir());
    let executable_found = executables.iter().any(|path| path.is_file());

    DetectedClient {
        id: ClientId::Codex,
        status: if application_found || executable_found {
            InstallationStatus::Installed
        } else {
            InstallationStatus::Unknown
        },
        detection_method: if application_found {
            Some(DetectionMethod::ApplicationBundle)
        } else if executable_found {
            Some(DetectionMethod::Executable)
        } else {
            None
        },
        version: None,
    }
}

pub fn detect_codex() -> DetectedClient {
    let mut applications = vec![PathBuf::from("/Applications/Codex.app")];

    let mut executables = vec![
        PathBuf::from("/opt/homebrew/bin/codex"),
        PathBuf::from("/usr/local/bin/codex"),
    ];

    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);

        applications.push(home.join("Applications/Codex.app"));
        executables.push(home.join(".local/bin/codex"));
        executables.push(home.join(".npm-global/bin/codex"));
    }

    detect_codex_at_paths(&applications, &executables)
}

fn detect_cursor_at_paths(applications: &[PathBuf], executables: &[PathBuf]) -> DetectedClient {
    let application_found = applications.iter().any(|path| path.is_dir());
    let executable_found = executables.iter().any(|path| path.is_file());

    DetectedClient {
        id: ClientId::Cursor,
        status: if application_found || executable_found {
            InstallationStatus::Installed
        } else {
            InstallationStatus::Unknown
        },
        detection_method: if application_found {
            Some(DetectionMethod::ApplicationBundle)
        } else if executable_found {
            Some(DetectionMethod::Executable)
        } else {
            None
        },
        version: None,
    }
}

pub fn detect_cursor() -> DetectedClient {
    let mut applications = vec![PathBuf::from("/Applications/Cursor.app")];

    let mut executables = vec![
        PathBuf::from("/opt/homebrew/bin/cursor"),
        PathBuf::from("/usr/local/bin/cursor"),
    ];

    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);

        applications.push(home.join("Applications/Cursor.app"));
        executables.push(home.join(".local/bin/cursor"));
        executables.push(home.join(".npm-global/bin/cursor"));
    }

    detect_cursor_at_paths(&applications, &executables)
}

fn detect_gemini_cli_at_paths(paths: &[PathBuf]) -> DetectedClient {
    let installed = paths.iter().any(|path| path.is_file());

    DetectedClient {
        id: ClientId::GeminiCli,
        status: if installed {
            InstallationStatus::Installed
        } else {
            InstallationStatus::Unknown
        },
        detection_method: if installed {
            Some(DetectionMethod::Executable)
        } else {
            None
        },
        version: None,
    }
}

pub fn detect_gemini_cli() -> DetectedClient {
    let mut locations = vec![
        PathBuf::from("/opt/homebrew/bin/gemini"),
        PathBuf::from("/usr/local/bin/gemini"),
    ];

    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);

        locations.push(home.join(".local/bin/gemini"));
        locations.push(home.join(".npm-global/bin/gemini"));
    }

    detect_gemini_cli_at_paths(&locations)
}

fn detect_antigravity_at_paths(
    applications: &[PathBuf],
    executables: &[PathBuf],
) -> DetectedClient {
    let application_found = applications.iter().any(|path| path.is_dir());
    let executable_found = executables.iter().any(|path| path.is_file());

    DetectedClient {
        id: ClientId::Antigravity,
        status: if application_found || executable_found {
            InstallationStatus::Installed
        } else {
            InstallationStatus::Unknown
        },
        detection_method: if application_found {
            Some(DetectionMethod::ApplicationBundle)
        } else if executable_found {
            Some(DetectionMethod::Executable)
        } else {
            None
        },
        version: None,
    }
}

pub fn detect_antigravity() -> DetectedClient {
    let mut applications = vec![
        PathBuf::from("/Applications/Antigravity.app"),
        PathBuf::from("/Applications/Google Antigravity.app"),
    ];

    let mut executables = vec![
        PathBuf::from("/opt/homebrew/bin/antigravity"),
        PathBuf::from("/usr/local/bin/antigravity"),
    ];

    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);

        applications.push(home.join("Applications/Antigravity.app"));
        applications.push(home.join("Applications/Google Antigravity.app"));
        executables.push(home.join(".local/bin/antigravity"));
        executables.push(home.join(".npm-global/bin/antigravity"));
    }

    detect_antigravity_at_paths(&applications, &executables)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antigravity_detects_desktop_application() {
        let result = detect_antigravity_at_paths(&[PathBuf::from(env!("CARGO_MANIFEST_DIR"))], &[]);

        assert_eq!(result.id, ClientId::Antigravity);
        assert_eq!(result.status, InstallationStatus::Installed);
        assert_eq!(
            result.detection_method,
            Some(DetectionMethod::ApplicationBundle)
        );
    }

    #[test]
    fn antigravity_detects_cli_executable() {
        let result = detect_antigravity_at_paths(&[], &[PathBuf::from("/bin/sh")]);

        assert_eq!(result.status, InstallationStatus::Installed);
        assert_eq!(result.detection_method, Some(DetectionMethod::Executable));
    }

    #[test]
    fn antigravity_prefers_desktop_when_both_exist() {
        let result = detect_antigravity_at_paths(
            &[PathBuf::from(env!("CARGO_MANIFEST_DIR"))],
            &[PathBuf::from("/bin/sh")],
        );

        assert_eq!(result.status, InstallationStatus::Installed);
        assert_eq!(
            result.detection_method,
            Some(DetectionMethod::ApplicationBundle)
        );
    }

    #[test]
    fn antigravity_reports_unknown_when_not_found() {
        let result = detect_antigravity_at_paths(
            &[PathBuf::from("/figcon-nonexistent-antigravity-app-9e6c2a")],
            &[PathBuf::from("/figcon-nonexistent-antigravity-cli-9e6c2a")],
        );

        assert_eq!(result.id, ClientId::Antigravity);
        assert_eq!(result.status, InstallationStatus::Unknown);
        assert_eq!(result.detection_method, None);
        assert!(result.version.is_none());
    }

    #[test]
    fn gemini_cli_detects_existing_executable() {
        let paths = vec![PathBuf::from("/bin/sh")];

        let result = detect_gemini_cli_at_paths(&paths);

        assert_eq!(result.id, ClientId::GeminiCli);
        assert_eq!(result.status, InstallationStatus::Installed);
        assert_eq!(result.detection_method, Some(DetectionMethod::Executable));
        assert!(result.version.is_none());
    }

    #[test]
    fn gemini_cli_reports_unknown_when_not_found() {
        let paths = vec![PathBuf::from("/figcon-test-nonexistent-gemini-cli-9e6c2a")];

        let result = detect_gemini_cli_at_paths(&paths);

        assert_eq!(result.id, ClientId::GeminiCli);
        assert_eq!(result.status, InstallationStatus::Unknown);
        assert_eq!(result.detection_method, None);
        assert!(result.version.is_none());
    }

    #[test]
    fn detects_existing_application_bundle() {
        let paths = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))];

        let result = detect_claude_desktop_at_paths(&paths);

        assert!(matches!(result.id, ClientId::ClaudeDesktop));
        assert!(matches!(result.status, InstallationStatus::Installed));
        assert!(matches!(
            result.detection_method,
            Some(DetectionMethod::ApplicationBundle)
        ));
    }

    #[test]
    fn returns_unknown_when_application_is_not_found() {
        let paths = vec![PathBuf::from(
            "/figcon-test-nonexistent-claude-desktop-9e6c2a",
        )];

        let result = detect_claude_desktop_at_paths(&paths);

        assert!(matches!(result.status, InstallationStatus::Unknown));
        assert!(result.detection_method.is_none());
        assert!(result.version.is_none());
    }

    #[test]
    fn detects_existing_claude_code_executable() {
        let paths = vec![PathBuf::from("/bin/sh")];

        let result = detect_claude_code_at_paths(&paths);

        assert!(matches!(result.id, ClientId::ClaudeCode));
        assert!(matches!(result.status, InstallationStatus::Installed));
        assert!(matches!(
            result.detection_method,
            Some(DetectionMethod::Executable)
        ));
    }

    #[test]
    fn returns_unknown_when_claude_code_is_missing() {
        let paths = vec![PathBuf::from("/figcon-test-nonexistent-claude-code-9e6c2a")];

        let result = detect_claude_code_at_paths(&paths);

        assert!(matches!(result.status, InstallationStatus::Unknown));
        assert!(result.detection_method.is_none());
        assert!(result.version.is_none());
    }

    #[test]
    fn detects_claude_code_through_symbolic_link() {
        use std::os::unix::fs::symlink;

        let unique = format!("figcon-claude-code-test-{}", std::process::id());
        let test_dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&test_dir).unwrap();

        let link = test_dir.join("claude");
        symlink("/bin/sh", &link).unwrap();

        let result = detect_claude_code_at_paths(&[link]);

        std::fs::remove_dir_all(&test_dir).unwrap();

        assert!(matches!(result.status, InstallationStatus::Installed));
        assert!(matches!(
            result.detection_method,
            Some(DetectionMethod::Executable)
        ));
    }

    #[test]
    fn detects_codex_desktop() {
        let apps = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))];
        let result = detect_codex_at_paths(&apps, &[]);

        assert!(matches!(result.id, ClientId::Codex));
        assert!(matches!(result.status, InstallationStatus::Installed));
        assert!(matches!(
            result.detection_method,
            Some(DetectionMethod::ApplicationBundle)
        ));
    }

    #[test]
    fn detects_codex_cli() {
        let executables = vec![PathBuf::from("/bin/sh")];
        let result = detect_codex_at_paths(&[], &executables);

        assert!(matches!(result.status, InstallationStatus::Installed));
        assert!(matches!(
            result.detection_method,
            Some(DetectionMethod::Executable)
        ));
    }

    #[test]
    fn prefers_codex_desktop_when_both_are_installed() {
        let apps = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))];
        let executables = vec![PathBuf::from("/bin/sh")];

        let result = detect_codex_at_paths(&apps, &executables);

        assert!(matches!(result.status, InstallationStatus::Installed));
        assert!(matches!(
            result.detection_method,
            Some(DetectionMethod::ApplicationBundle)
        ));
    }

    #[test]
    fn returns_unknown_when_codex_is_not_found() {
        let result = detect_codex_at_paths(&[], &[]);

        assert!(matches!(result.id, ClientId::Codex));
        assert!(matches!(result.status, InstallationStatus::Unknown));
        assert!(result.detection_method.is_none());
        assert!(result.version.is_none());
    }

    #[test]
    fn cursor_detects_desktop_application() {
        let applications = vec![PathBuf::from("/bin")];
        let executables = vec![];

        let result = detect_cursor_at_paths(&applications, &executables);

        assert_eq!(result.id, ClientId::Cursor);
        assert_eq!(result.status, InstallationStatus::Installed);
        assert_eq!(
            result.detection_method,
            Some(DetectionMethod::ApplicationBundle)
        );
    }

    #[test]
    fn cursor_detects_cli_executable() {
        let applications = vec![];
        let executables = vec![PathBuf::from("/bin/sh")];

        let result = detect_cursor_at_paths(&applications, &executables);

        assert_eq!(result.id, ClientId::Cursor);
        assert_eq!(result.status, InstallationStatus::Installed);
        assert_eq!(result.detection_method, Some(DetectionMethod::Executable));
    }

    #[test]
    fn cursor_prefers_desktop_when_both_exist() {
        let applications = vec![PathBuf::from("/bin")];
        let executables = vec![PathBuf::from("/bin/sh")];

        let result = detect_cursor_at_paths(&applications, &executables);

        assert_eq!(result.status, InstallationStatus::Installed);
        assert_eq!(
            result.detection_method,
            Some(DetectionMethod::ApplicationBundle)
        );
    }

    #[test]
    fn cursor_reports_unknown_when_not_found() {
        let applications = vec![PathBuf::from("/nonexistent-figcon-cursor-app")];
        let executables = vec![PathBuf::from("/nonexistent-figcon-cursor-cli")];

        let result = detect_cursor_at_paths(&applications, &executables);

        assert_eq!(result.id, ClientId::Cursor);
        assert_eq!(result.status, InstallationStatus::Unknown);
        assert_eq!(result.detection_method, None);
    }
}
