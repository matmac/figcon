use std::path::Path;
use std::time::Duration;

use super::manifest::NODE_VERSION;

pub async fn validate_node_runtime(runtime: &Path) -> Result<(), String> {
    let executable = runtime.join("bin/node");

    if !executable.is_file() {
        return Err("Node.js executable is missing".into());
    }

    let mut command = tokio::process::Command::new(&executable);
    command.arg("--version").kill_on_drop(true);

    let output = tokio::time::timeout(Duration::from_secs(10), command.output())
        .await
        .map_err(|_| "Node.js version check timed out")?
        .map_err(|_| "Failed to launch Node.js runtime")?;

    if !output.status.success() {
        return Err("Node.js version check failed".into());
    }

    let version = String::from_utf8(output.stdout).map_err(|_| "Invalid Node.js version output")?;

    if version.trim() != format!("v{NODE_VERSION}") {
        return Err("Node.js runtime version mismatch".into());
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
            "Node.js executable is missing"
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
    async fn accepts_expected_runtime_version() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("bin");
        std::fs::create_dir(&bin).unwrap();

        let executable = bin.join("node");

        std::fs::write(&executable, format!("#!/bin/sh\necho v{NODE_VERSION}\n")).unwrap();

        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();

        validate_node_runtime(temp.path()).await.unwrap();
    }
}
