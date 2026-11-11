//! Run with an isolated process home: never change the test runner's HOME.
#![cfg(target_os = "macos")]
use super::{api_models::*, deadline::Deadline, progress::Progress};
use std::{
    fs,
    sync::{Arc, Mutex},
    time::Duration,
};

#[test]
fn external_configurations_remain_byte_identical() {
    let home = tempfile::tempdir().unwrap();
    let files = [
        ".claude.json",
        "Library/Application Support/Claude/claude_desktop_config.json",
        ".codex/config.toml",
        ".cursor/mcp.json",
        "Library/Application Support/Code/User/mcp.json",
    ];
    for name in files {
        let path = home.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let content: &[u8] = if name.ends_with(".toml") {
            b"# synthetic external configuration\n[mcp_servers.example]\ncommand = 'do-not-run'\n"
        } else {
            b"{\n  \"mcpServers\": {\"example\": {\"command\": \"do-not-run\"}},\n  \"synthetic\": true\n}\n"
        };
        fs::write(path, content).unwrap();
    }
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "installer::external_config_tests::isolated_home_child",
            "--nocapture",
        ])
        .env("FIGCON_SYNTHETIC_CONFIG_HOME", home.path())
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .status()
        .unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn isolated_home_child() {
    let Some(home) = std::env::var_os("FIGCON_SYNTHETIC_CONFIG_HOME") else {
        return;
    };
    let home = std::path::PathBuf::from(home);
    assert_eq!(std::env::var_os("HOME").unwrap(), home.as_os_str());
    let names = [
        ".claude.json",
        "Library/Application Support/Claude/claude_desktop_config.json",
        ".codex/config.toml",
        ".cursor/mcp.json",
        "Library/Application Support/Code/User/mcp.json",
    ];
    let before: Vec<_> = names
        .iter()
        .map(|p| fs::read(home.join(p)).unwrap())
        .collect();
    for action in ["success", "failed", "cancelled"] {
        let base = tempfile::tempdir_in(&home).unwrap();
        let paths = super::mcp::tests::macos::setup(base.path(), action);
        let holder = Arc::new(Mutex::new(None::<Deadline>));
        let target = holder.clone();
        let progress = Arc::new(Progress::new(
            super::attempt::new_id().unwrap(),
            Component::Mcp,
            Arc::new(move |event| {
                if action == "cancelled" && event.stage == Stage::Installing {
                    assert_eq!(
                        target.lock().unwrap().as_ref().unwrap().cancel(),
                        CancellationDisposition::Requested
                    );
                }
            }),
        ));
        let deadline = Deadline::observed(Duration::from_secs(30), Some(progress));
        *holder.lock().unwrap() = Some(deadline.clone());
        let installed = paths.mcp.clone();
        let result = super::deadline::run_with(deadline, move || async move {
            super::mcp::install_mcp(&paths).await
        })
        .await;
        match action {
            "success" => {
                result.unwrap();
                assert!(installed.is_dir());
            }
            "failed" => {
                assert!(result.unwrap_err().contains("Installer process failed"));
                assert!(!installed.exists());
            }
            _ => {
                assert!(result.unwrap_err().contains("cancelled"));
                assert!(!installed.exists());
            }
        }
        for (name, original) in names.iter().zip(&before) {
            assert_eq!(
                fs::read(home.join(name)).unwrap(),
                *original,
                "external configuration changed during {action}: {name}"
            );
        }
    }
}
