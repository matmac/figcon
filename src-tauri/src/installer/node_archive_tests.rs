use std::fs;
use std::path::PathBuf;

use super::checksum::verify_sha256;
use super::manifest::{NODE_VERSION, node_archive};
use super::runtime_aliases::APPROVED;

fn verified_archive(architecture: &str) -> PathBuf {
    let variable = match architecture {
        "arm64" => "FIGCON_TEST_NODE_ARM64_ARCHIVE",
        "x64" => "FIGCON_TEST_NODE_X64_ARCHIVE",
        _ => panic!("Unsupported test architecture"),
    };
    let archive = PathBuf::from(
        std::env::var_os(variable).expect("Supply the official pinned archive fixture"),
    );
    assert!(
        verify_sha256(&archive, node_archive(architecture).unwrap().1).unwrap(),
        "Pinned Node archive checksum mismatch"
    );
    archive
}

#[cfg(unix)]
#[test]
#[ignore = "requires official arm64 and x64 archive fixtures; checks hashes before extraction"]
fn real_node_archives_extract_without_executing_x64() {
    use std::io::Read;
    for (architecture, cpu) in [("arm64", 0x0100000cu32), ("x64", 0x01000007u32)] {
        let archive = verified_archive(architecture);
        let temp = tempfile::tempdir().unwrap();
        let runtime = temp.path().join("runtime");
        super::extract::extract_node_archive(&archive, &runtime, NODE_VERSION, architecture)
            .unwrap();
        super::runtime_aliases::validate_runtime_tree(&runtime).unwrap();
        for alias in &APPROVED {
            assert_eq!(
                fs::read_link(runtime.join(alias.name)).unwrap().as_os_str(),
                std::ffi::OsStr::new(alias.link_target)
            );
            assert_eq!(
                runtime.join(alias.name).canonicalize().unwrap(),
                runtime.join(alias.file_target).canonicalize().unwrap()
            );
        }
        let mut header = [0; 8];
        fs::File::open(runtime.join("bin/node"))
            .unwrap()
            .read_exact(&mut header)
            .unwrap();
        assert_eq!(&header[..4], &[0xcf, 0xfa, 0xed, 0xfe]);
        assert_eq!(u32::from_le_bytes(header[4..].try_into().unwrap()), cpu);
        // In particular, never launch x64 node, npm, or its executable aliases.
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[tokio::test]
#[ignore = "requires the official arm64 archive fixture; runs real private Node.js and npm"]
async fn real_node_archive_first_run_prepares_and_activates() {
    let archive = verified_archive("arm64");
    let temp = tempfile::tempdir().unwrap();
    let paths = super::paths::InstallerPaths::new(
        temp.path()
            .join("missing-app-data")
            .join("figcon-installer"),
    );
    assert!(!paths.root.parent().unwrap().exists());
    let (runtime, attempt) = super::runtime::prepare_node_runtime_attempt_with_downloader(
        &paths,
        "arm64",
        move |architecture, destination| {
            Box::pin(async move {
                let checksum = node_archive(architecture)
                    .ok_or("Unsupported architecture")?
                    .1;
                if !verify_sha256(&archive, checksum).map_err(|_| "Cannot verify archive")? {
                    return Err("Fixture archive checksum mismatch".into());
                }
                fs::copy(&archive, destination).map_err(|_| "Cannot copy verified archive")?;
                if !verify_sha256(destination, checksum)
                    .map_err(|_| "Cannot verify copied archive")?
                {
                    return Err("Copied archive checksum mismatch".into());
                }
                Ok(())
            })
        },
    )
    .await
    .unwrap();
    // Preparation invokes the actual private Node.js and npm version probes.
    super::validate::validate_node_runtime(&runtime)
        .await
        .unwrap();
    super::activate::activate_node_runtime(&paths, &runtime)
        .await
        .unwrap();
    attempt
        .checkpoint(super::attempt::Lifecycle::Activated)
        .unwrap();
    attempt.cleanup().unwrap();
    super::validate::validate_node_runtime(&paths.runtime)
        .await
        .unwrap();
    assert!(
        paths
            .runtime
            .join("bin/npm")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_dir(&paths.staging).unwrap().count(), 1);
    assert!(!paths.mcp.exists());
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[tokio::test]
#[ignore = "downloads the pinned real arm64 archive through the production installer"]
async fn real_node_production_network_first_run() {
    let temp = tempfile::tempdir().unwrap();
    let paths = super::paths::InstallerPaths::new(
        temp.path()
            .join("missing-app-data")
            .join("figcon-installer"),
    );
    super::install::install_node_runtime(&paths, "arm64")
        .await
        .unwrap();
    super::validate::validate_node_runtime(&paths.runtime)
        .await
        .unwrap();
    assert_eq!(fs::read_dir(&paths.staging).unwrap().count(), 1);
    assert!(!paths.mcp.exists());
}
