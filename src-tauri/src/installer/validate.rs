use std::path::Path;
use std::time::Duration;

use super::manifest::NODE_VERSION;
use super::mcp_fs::real_file;
use super::npm::NPM_CLI;

pub(super) fn bundled_npm_version(runtime: &Path) -> Result<String, String> {
    let metadata = real_file(runtime, Path::new("lib/node_modules/npm/package.json"))?;
    let bytes = super::mcp_fs::read_regular(&metadata, 1024 * 1024)?;
    let npm: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid bundled npm metadata")?;
    let version = npm["version"]
        .as_str()
        .filter(|v| super::mcp_contract::exact_version(v))
        .ok_or("Bundled npm version is invalid")?;
    if npm["name"] != "npm" {
        return Err("Bundled npm identity is invalid".into());
    }
    Ok(version.to_owned())
}

/// Read-only structural inspection; never executes a possibly damaged runtime.
/// The installer separately probes executable versions after verified download.
pub(super) fn inspect_node_runtime(runtime: &Path) -> Result<(), String> {
    super::runtime_aliases::validate_runtime_tree(runtime)?;
    let executable = real_file(runtime, Path::new("bin/node"))?;
    let metadata =
        std::fs::symlink_metadata(executable).map_err(|_| "Cannot inspect Node.js executable")?;
    if metadata.len() == 0 {
        return Err("Node.js executable is empty".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("Node.js executable is not executable".into());
        }
    }
    let npm = real_file(runtime, Path::new(NPM_CLI))?;
    if std::fs::symlink_metadata(npm)
        .map_err(|_| "Cannot inspect npm CLI")?
        .len()
        == 0
    {
        return Err("Private npm CLI is empty".into());
    }
    bundled_npm_version(runtime)?;
    Ok(())
}

pub async fn validate_node_runtime(runtime: &Path) -> Result<(), String> {
    let executable = real_file(runtime, Path::new("bin/node"))
        .map_err(|_| "Node.js executable is missing or unsafe")?;
    super::runtime_aliases::validate_runtime_tree(runtime)?;
    let mut command = tokio::process::Command::new(&executable);
    command.arg("--version").current_dir(runtime).env_clear();
    let output = super::process::run(&mut command, Duration::from_secs(10), true)
        .await
        .map_err(|_| "Node.js version check failed")?;
    if String::from_utf8_lossy(&output).trim() != format!("v{NODE_VERSION}") {
        return Err("Node.js runtime version mismatch".into());
    }

    let npm_cli = real_file(runtime, Path::new(NPM_CLI))
        .map_err(|_| "Private npm CLI is missing or unsafe")?;
    let version = bundled_npm_version(runtime)?;

    // --version exits without installation. No ambient NODE_OPTIONS, npm
    // configuration, credentials, or user home are inherited. The explicitly
    // absent config paths cannot resolve to a user's existing configuration.
    for name in [".figcon-no-user-config", ".figcon-no-global-config"] {
        if std::fs::symlink_metadata(runtime.join(name)).is_ok() {
            return Err("Unexpected runtime validation configuration".into());
        }
    }
    let mut command = tokio::process::Command::new(executable);
    command
        .arg(npm_cli)
        .args([
            "--version",
            "--ignore-scripts",
            "--logs-max=0",
            "--update-notifier=false",
        ])
        .current_dir(runtime)
        .env_clear()
        .env("HOME", runtime)
        .env("PATH", runtime.join("bin"))
        .env(
            "npm_config_userconfig",
            runtime.join(".figcon-no-user-config"),
        )
        .env(
            "npm_config_globalconfig",
            runtime.join(".figcon-no-global-config"),
        )
        .env("npm_config_cache", runtime.join(".figcon-no-cache"));
    let output = super::process::run(&mut command, Duration::from_secs(10), true)
        .await
        .map_err(|_| "Private npm version check failed")?;
    if String::from_utf8_lossy(&output).trim() != version {
        return Err("Private npm version mismatch".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_missing_executable() {
        let temp = tempfile::tempdir().unwrap();

        assert_eq!(
            validate_node_runtime(temp.path()).await.unwrap_err(),
            "Node.js executable is missing or unsafe"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rejects_incorrect_runtime_version() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("bin");
        std::fs::create_dir(&bin).unwrap();

        let executable = bin.join("node");

        std::fs::write(&executable, b"#!/bin/sh\necho v0.0.0\n").unwrap();

        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();

        assert_eq!(
            validate_node_runtime(temp.path()).await.unwrap_err(),
            "Node.js runtime version mismatch"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn accepts_expected_runtime_version_and_private_npm_without_bin_link() {
        let temp = tempfile::tempdir().unwrap();
        super::super::test_support::write_runtime(temp.path());
        assert!(!temp.path().join("bin/npm").exists());
        validate_node_runtime(temp.path()).await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rejects_missing_or_broken_private_npm() {
        let temp = tempfile::tempdir().unwrap();
        super::super::test_support::write_runtime(temp.path());
        std::fs::remove_file(temp.path().join(NPM_CLI)).unwrap();
        assert_eq!(
            validate_node_runtime(temp.path()).await.unwrap_err(),
            "Private npm CLI is missing or unsafe"
        );
        std::fs::write(temp.path().join(NPM_CLI), b"fixture").unwrap();
        std::fs::write(
            temp.path().join("lib/node_modules/npm/package.json"),
            br#"{"name":"npm","version":"999.0.0"}"#,
        )
        .unwrap();
        assert_eq!(
            validate_node_runtime(temp.path()).await.unwrap_err(),
            "Private npm version mismatch"
        );
    }
}
