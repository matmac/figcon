#[cfg(test)]
use std::fs;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use super::activate::activate_node_runtime;
use super::paths::InstallerPaths;

/// Downloads, validates and activates Figcon's private Node.js runtime.
pub async fn install_node_runtime(
    paths: &InstallerPaths,
    architecture: &str,
) -> Result<(), String> {
    let paths = paths.clone();
    let architecture = architecture.to_owned();
    super::deadline::run(move || async move {
        install_node_runtime_with_downloader(&paths, &architecture, |arch, dest| {
            Box::pin(super::download::download_node_archive(arch, dest))
        })
        .await
    })
    .await
}

async fn install_node_runtime_with_downloader<F>(
    paths: &InstallerPaths,
    architecture: &str,
    downloader: F,
) -> Result<(), String>
where
    F: for<'a> FnOnce(&'a str, &'a Path) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>>,
{
    let (staged_runtime, attempt) = super::runtime::prepare_node_runtime_attempt_with_downloader(
        paths,
        architecture,
        downloader,
    )
    .await?;

    attempt
        .verify()
        .map_err(|error| format!("Staging ownership changed: {error}"))?;
    attempt.verify_checkpoint()?;
    attempt.state(super::attempt::Lifecycle::Activating)?;
    let activation = activate_node_runtime(paths, &staged_runtime)
        .await
        .map_err(|error| format!("Failed to activate Node.js runtime: {error}"));
    let activation = activation.and_then(|()| {
        attempt
            .checkpoint(super::attempt::Lifecycle::Activated)
            .map_err(|error| format!("Runtime activated but durable checkpoint failed: {error}"))
    });

    match (activation, attempt.cleanup()) {
        (Ok(()), Ok(_)) => Ok(()),
        (Err(error), Ok(_)) => Err(format!(
            "{error}; staging retained for manual review; automatic cleanup disabled"
        )),
        (Ok(()), Err(error)) => Err(format!(
            "Runtime activated, but staging cleanup refused or failed: {error}"
        )),
        (Err(error), Err(cleanup)) => Err(format!(
            "{error}; staging cleanup refused or failed: {cleanup}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::installer::manifest::NODE_VERSION;

    #[cfg(unix)]
    fn create_valid_archive(destination: &Path) -> Result<(), String> {
        let file = fs::File::create(destination).map_err(|e| e.to_string())?;
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);

        let contents = super::super::test_support::node_script();

        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();

        archive
            .append_data(
                &mut header,
                format!("node-v{NODE_VERSION}-darwin-arm64/bin/node"),
                contents.as_bytes(),
            )
            .map_err(|e| e.to_string())?;

        super::super::test_support::append_npm(
            &mut archive,
            &format!("node-v{NODE_VERSION}-darwin-arm64"),
        );

        archive
            .into_inner()
            .map_err(|e| e.to_string())?
            .finish()
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn installs_runtime_and_reports_retained_staging() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));

        install_node_runtime_with_downloader(&paths, "arm64", |_architecture, destination| {
            Box::pin(async move { create_valid_archive(destination) })
        })
        .await
        .unwrap();

        assert!(paths.runtime.join("bin/node").is_file());
        assert_eq!(fs::read_dir(&paths.staging).unwrap().count(), 1);
        assert!(!paths.mcp.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn preserves_existing_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));

        super::super::root::prepare_installation_root(&paths.root).unwrap();
        fs::create_dir(&paths.runtime).unwrap();
        fs::write(paths.runtime.join("marker"), b"existing").unwrap();

        let result =
            install_node_runtime_with_downloader(&paths, "arm64", |_architecture, destination| {
                Box::pin(async move { create_valid_archive(destination) })
            })
            .await;

        assert!(
            result.unwrap_err().starts_with(
                "Failed to activate Node.js runtime: An active runtime already exists"
            ),
            "A valid staged runtime must reach activation"
        );
        assert_eq!(fs::read(paths.runtime.join("marker")).unwrap(), b"existing");
        assert_eq!(fs::read_dir(&paths.staging).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn retains_failed_download_attempt() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));

        let result =
            install_node_runtime_with_downloader(&paths, "arm64", |_architecture, _destination| {
                Box::pin(async { Err("Simulated download failure".to_string()) })
            })
            .await;

        assert!(
            result
                .unwrap_err()
                .contains("Simulated download failure; staging retained")
        );
        assert!(!paths.runtime.exists());
        assert_eq!(fs::read_dir(&paths.staging).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn rejects_unsupported_architecture_without_creating_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));

        let result = install_node_runtime_with_downloader(
            &paths,
            "unsupported",
            |_architecture, _destination| {
                Box::pin(async { Err("Downloader should not run".to_string()) })
            },
        )
        .await;

        assert_eq!(result.unwrap_err(), "Unsupported macOS architecture");
        assert!(!paths.root.exists());
    }
}
