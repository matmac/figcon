use std::path::{Path, PathBuf};

use super::download::download_node_archive;
use super::extract::extract_node_archive;
use super::manifest::NODE_VERSION;
use super::paths::InstallerPaths;
use super::validate::validate_node_runtime;
use super::workspace::prepare_installation_workspace;

/// Downloads and extracts a pinned Node.js runtime into private staging.
///
/// Does not modify the active runtime installation.
pub async fn prepare_node_runtime(
    paths: &InstallerPaths,
    architecture: &str,
) -> Result<PathBuf, String> {
    prepare_node_runtime_with_downloader(paths, architecture, |arch, dest| {
        Box::pin(download_node_archive(arch, dest))
    })
    .await
}

pub(super) async fn prepare_node_runtime_with_downloader<F>(
    paths: &InstallerPaths,
    architecture: &str,
    downloader: F,
) -> Result<PathBuf, String>
where
    F: for<'a> FnOnce(
        &'a str,
        &'a Path,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), String>> + 'a>,
    >,
{
    prepare_node_runtime_attempt_with_downloader(paths, architecture, downloader)
        .await
        .map(|(runtime, _attempt)| runtime)
}

pub(super) async fn prepare_node_runtime_attempt_with_downloader<F>(
    paths: &InstallerPaths,
    architecture: &str,
    downloader: F,
) -> Result<(PathBuf, super::cleanup::StagingAttempt), String>
where
    F: for<'a> FnOnce(
        &'a str,
        &'a Path,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), String>> + 'a>,
    >,
{
    if !matches!(architecture, "arm64" | "x64") {
        return Err("Unsupported macOS architecture".into());
    }

    let staging = prepare_installation_workspace(paths)
        .map_err(|_| "Failed to prepare installation workspace")?;

    let attempt = super::cleanup::StagingAttempt::capture(paths, staging)
        .map_err(|error| format!("Failed to capture staging ownership: {error}"))?;
    let result = prepare_in_staging(&attempt, architecture, downloader).await;
    match result {
        Ok(runtime) => {
            attempt
                .verify()
                .map_err(|error| format!("Staging ownership changed: {error}"))?;
            Ok((runtime, attempt))
        }
        Err(error) => match attempt.cleanup() {
            Ok(()) => Err(error),
            Err(cleanup) => Err(format!(
                "{error}; staging cleanup refused or failed: {cleanup}"
            )),
        },
    }
}

async fn prepare_in_staging<F>(
    attempt: &super::cleanup::StagingAttempt,
    architecture: &str,
    downloader: F,
) -> Result<PathBuf, String>
where
    F: for<'a> FnOnce(
        &'a str,
        &'a Path,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), String>> + 'a>,
    >,
{
    let archive = attempt.path().join("node.tar.gz");
    let runtime = attempt.path().join("runtime");

    downloader(architecture, &archive).await?;
    attempt
        .verify()
        .map_err(|error| format!("Staging ownership changed: {error}"))?;

    extract_node_archive(&archive, &runtime, NODE_VERSION, architecture)?;

    validate_node_runtime(&runtime).await?;

    attempt
        .verify()
        .map_err(|error| format!("Staging ownership changed: {error}"))?;
    std::fs::remove_file(&archive).map_err(|_| "Failed to remove downloaded archive")?;

    Ok(runtime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_attempt_does_not_delete_replacement_staging() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));
        let result = prepare_node_runtime_with_downloader(&paths, "arm64", |_, destination| {
            Box::pin(async move {
                let attempt = destination.parent().unwrap();
                std::fs::rename(attempt, attempt.with_extension("original")).unwrap();
                std::fs::create_dir(attempt).unwrap();
                std::fs::write(attempt.join("marker"), b"replacement").unwrap();
                Err("Simulated download failure".into())
            })
        })
        .await;
        let error = result.unwrap_err();
        assert!(error.contains("Simulated download failure"));
        assert!(error.contains("staging cleanup refused or failed"));
        let replacement = std::fs::read_dir(&paths.staging)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.join("marker").exists())
            .unwrap();
        assert_eq!(
            std::fs::read(replacement.join("marker")).unwrap(),
            b"replacement"
        );
        assert_eq!(std::fs::read_dir(&paths.staging).unwrap().count(), 2);
    }

    #[tokio::test]
    async fn rejects_unsupported_architecture_without_creating_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));

        let result = prepare_node_runtime(&paths, "unsupported").await;

        assert_eq!(result.unwrap_err(), "Unsupported macOS architecture");

        assert!(!paths.root.exists());
        assert!(!paths.runtime.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cleans_up_after_failed_download() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));

        let staging = prepare_installation_workspace(&paths).unwrap();

        let attempt =
            super::super::cleanup::StagingAttempt::capture(&paths, staging.clone()).unwrap();
        let result = prepare_in_staging(&attempt, "arm64", |_architecture, _destination| {
            Box::pin(async { Err("Simulated download failure".to_string()) })
        })
        .await;

        assert_eq!(result.unwrap_err(), "Simulated download failure");

        assert!(!paths.runtime.exists());
        assert!(!paths.mcp.exists());

        std::fs::remove_dir_all(&staging).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn automatically_cleans_up_failed_download() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));

        let result =
            prepare_node_runtime_with_downloader(&paths, "arm64", |_architecture, destination| {
                Box::pin(async move {
                    std::fs::write(destination, b"partial archive")
                        .map_err(|error| error.to_string())?;

                    Err("Simulated download failure".to_string())
                })
            })
            .await;

        assert_eq!(result.unwrap_err(), "Simulated download failure");

        assert!(!paths.runtime.exists());
        assert!(!paths.mcp.exists());

        assert_eq!(
            std::fs::read_dir(&paths.staging).unwrap().count(),
            0,
            "Failed installation must remove its staging directory"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn automatically_cleans_up_invalid_archive() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));

        let result =
            prepare_node_runtime_with_downloader(&paths, "arm64", |_architecture, destination| {
                Box::pin(async move {
                    std::fs::write(destination, b"not a valid tar.gz archive")
                        .map_err(|error| error.to_string())
                })
            })
            .await;

        assert!(result.is_err(), "Invalid archive must be rejected");

        assert!(!paths.runtime.exists());
        assert!(!paths.mcp.exists());

        assert_eq!(
            std::fs::read_dir(&paths.staging).unwrap().count(),
            0,
            "Failed extraction must remove its staging directory"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn prepares_valid_runtime_from_local_archive() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));

        let result =
            prepare_node_runtime_with_downloader(&paths, "arm64", |_architecture, destination| {
                Box::pin(async move {
                    let file =
                        std::fs::File::create(destination).map_err(|error| error.to_string())?;

                    let encoder =
                        flate2::write::GzEncoder::new(file, flate2::Compression::default());

                    let mut archive = tar::Builder::new(encoder);
                    let contents = super::super::test_support::node_script();

                    let mut header = tar::Header::new_gnu();
                    header.set_size(contents.len() as u64);
                    header.set_mode(0o755);
                    header.set_cksum();

                    archive
                        .append_data(
                            &mut header,
                            format!("node-v{}-darwin-arm64/bin/node", NODE_VERSION),
                            contents.as_bytes(),
                        )
                        .map_err(|error| error.to_string())?;

                    super::super::test_support::append_npm(
                        &mut archive,
                        &format!("node-v{NODE_VERSION}-darwin-arm64"),
                    );
                    let encoder = archive.into_inner().map_err(|error| error.to_string())?;

                    encoder.finish().map_err(|error| error.to_string())?;

                    Ok(())
                })
            })
            .await
            .unwrap();

        assert_eq!(
            std::fs::read(result.join("bin/node")).unwrap(),
            super::super::test_support::node_script().as_bytes()
        );

        assert!(!result.parent().unwrap().join("node.tar.gz").exists());
        assert!(!paths.runtime.exists());
        assert!(!paths.mcp.exists());

        use std::os::unix::fs::PermissionsExt;

        assert_ne!(
            std::fs::metadata(result.join("bin/node"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111,
            0
        );
    }
}
