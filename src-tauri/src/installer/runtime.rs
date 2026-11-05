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

async fn prepare_node_runtime_with_downloader<F>(
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
    if !matches!(architecture, "arm64" | "x64") {
        return Err("Unsupported macOS architecture".into());
    }

    let staging = prepare_installation_workspace(paths)
        .map_err(|_| "Failed to prepare installation workspace")?;

    let result = prepare_in_staging(&staging, architecture, downloader).await;

    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }

    result
}

async fn prepare_in_staging<F>(
    staging: &Path,
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
    let archive = staging.join("node.tar.gz");
    let runtime = staging.join("runtime");

    downloader(architecture, &archive).await?;

    extract_node_archive(&archive, &runtime, NODE_VERSION, architecture)?;

    validate_node_runtime(&runtime).await?;

    std::fs::remove_file(&archive).map_err(|_| "Failed to remove downloaded archive")?;

    Ok(runtime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_unsupported_architecture_without_creating_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));

        let result = prepare_node_runtime(&paths, "unsupported").await;

        assert_eq!(result.unwrap_err(), "Unsupported macOS architecture");

        assert!(!paths.root.exists());
        assert!(!paths.runtime.exists());
    }

    #[tokio::test]
    async fn cleans_up_after_failed_download() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));

        let staging = prepare_installation_workspace(&paths).unwrap();

        let result = prepare_in_staging(&staging, "arm64", |_architecture, _destination| {
            Box::pin(async { Err("Simulated download failure".to_string()) })
        })
        .await;

        assert_eq!(result.unwrap_err(), "Simulated download failure");

        assert!(!paths.runtime.exists());
        assert!(!paths.mcp.exists());

        std::fs::remove_dir_all(&staging).unwrap();
    }

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
                    let contents = format!("#!/bin/sh\necho v{NODE_VERSION}\n");

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

                    let encoder = archive.into_inner().map_err(|error| error.to_string())?;

                    encoder.finish().map_err(|error| error.to_string())?;

                    Ok(())
                })
            })
            .await
            .unwrap();

        assert_eq!(
            std::fs::read(result.join("bin/node")).unwrap(),
            format!("#!/bin/sh\necho v{NODE_VERSION}\n").as_bytes()
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
