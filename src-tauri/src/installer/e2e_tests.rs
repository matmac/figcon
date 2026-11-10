//! Opt-in network test: all installation work uses production entry points.
//! No application launch, external client discovery, or MCP server execution.
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::cleanup::StagingAttempt;
use super::manifest::{MCP_VERSION, NODE_VERSION};
use super::models::InstallationState;
use super::paths::InstallerPaths;

#[derive(Debug, PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
    mode: u32,
    modified: (i64, i64),
    changed: (i64, i64),
    contents: Vec<u8>,
}

// Hash all regular files and retain file/directory/symlink identities: replacing
// files with byte-identical copies is an overwrite and must fail this check.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Identity> {
    use std::os::unix::ffi::OsStrExt;
    let mut pending = vec![root.to_path_buf()];
    let mut found = BTreeMap::new();
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path).unwrap();
        let contents = if metadata.file_type().is_symlink() {
            fs::read_link(&path)
                .unwrap()
                .as_os_str()
                .as_bytes()
                .to_vec()
        } else if metadata.is_file() {
            let mut file = fs::File::open(&path).unwrap();
            let mut hash = Sha256::new();
            let mut buffer = [0; 64 * 1024];
            loop {
                let count = file.read(&mut buffer).unwrap();
                if count == 0 {
                    break;
                }
                hash.update(&buffer[..count]);
            }
            hash.finalize().to_vec()
        } else {
            assert!(metadata.is_dir());
            for entry in fs::read_dir(&path).unwrap() {
                pending.push(entry.unwrap().path());
            }
            Vec::new()
        };
        found.insert(
            path.strip_prefix(root).unwrap().to_path_buf(),
            Identity {
                device: metadata.dev(),
                inode: metadata.ino(),
                mode: metadata.permissions().mode(),
                modified: (metadata.mtime(), metadata.mtime_nsec()),
                changed: (metadata.ctime(), metadata.ctime_nsec()),
                contents,
            },
        );
    }
    found
}

fn assert_contained(paths: &InstallerPaths) {
    let root = paths.root.canonicalize().unwrap();
    let mut pending = vec![paths.runtime.clone(), paths.mcp.clone()];
    while let Some(path) = pending.pop() {
        assert!(
            path.canonicalize().unwrap().starts_with(&root),
            "Installed entry escaped isolated root"
        );
        let metadata = fs::symlink_metadata(&path).unwrap();
        if metadata.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                pending.push(entry.unwrap().path());
            }
        }
    }
}

fn assert_status(paths: &InstallerPaths, expected: InstallationState) {
    let status = super::status::inspect_installation(paths);
    assert_eq!(status.state, expected);
    assert_eq!(
        status.installed_version.as_deref(),
        (expected == InstallationState::Installed).then_some(MCP_VERSION)
    );
    assert_eq!(status.target_version, MCP_VERSION);
}

async fn versions(paths: &InstallerPaths) -> (String, String) {
    let executable = paths.runtime.join("bin/node");
    let mut node = tokio::process::Command::new(&executable);
    node.arg("--version")
        .env_clear()
        .current_dir(&paths.runtime);
    assert_eq!(node.as_std().get_program(), executable.as_os_str());
    let node = String::from_utf8(
        super::process::run(&mut node, Duration::from_secs(10), true)
            .await
            .unwrap(),
    )
    .unwrap();

    let attempt = super::workspace::prepare_installation_workspace(paths).unwrap();
    let attempt = StagingAttempt::capture(paths, attempt).unwrap();
    let project = attempt.path().join("probe");
    super::mcp_fs::private_directory(&project).unwrap();
    let npm = super::npm::NpmEnvironment::create(attempt.path(), &paths.runtime, &project).unwrap();
    let mut command = npm.command("--version").unwrap();
    assert_eq!(command.as_std().get_program(), executable.as_os_str());
    assert_eq!(
        command.as_std().get_args().next().unwrap(),
        paths.runtime.join(super::npm::NPM_CLI)
    );
    let search_path = command
        .as_std()
        .get_envs()
        .find(|(key, _)| *key == "PATH")
        .unwrap()
        .1
        .unwrap();
    assert_eq!(search_path, paths.runtime.join("bin"));
    let npm = String::from_utf8(
        super::process::run(&mut command, Duration::from_secs(10), true)
            .await
            .unwrap(),
    )
    .unwrap();
    attempt
        .checkpoint(super::attempt::Lifecycle::Prepared)
        .unwrap();
    attempt.cleanup().unwrap();
    (node.trim().to_owned(), npm.trim().to_owned())
}

#[tokio::test]
#[ignore = "opt-in macOS arm64 end-to-end test; downloads real Node.js and pinned npm packages"]
async fn clean_managed_installation_e2e() {
    let temporary = tempfile::tempdir().unwrap();
    let paths = InstallerPaths::new(
        temporary
            .path()
            .join("new-app-data")
            .join("figcon-installer"),
    );
    assert!(!paths.root.exists());
    assert_status(&paths, InstallationState::NotInstalled);
    assert!(
        !paths.root.exists(),
        "Status must not initialize or modify the root"
    );
    println!("E2E clean status: not-installed");

    // Actual HTTPS download + pinned SHA-256, extraction, private version
    // probes, atomic activation, and attempt-owned cleanup. No seeded fixture.
    super::install::install_node_runtime(&paths, "arm64")
        .await
        .unwrap();
    super::validate::validate_node_runtime(&paths.runtime)
        .await
        .unwrap();
    assert_status(&paths, InstallationState::NotInstalled);
    let (node, npm) = versions(&paths).await;
    assert_eq!(node, format!("v{NODE_VERSION}"));
    assert_eq!(
        npm,
        super::validate::bundled_npm_version(&paths.runtime).unwrap()
    );
    println!("E2E actual runtime: Node.js {node}, npm {npm}; runtime-only status: not-installed");
    let runtime_before = snapshot(&paths.runtime);

    // An incomplete active directory is an error, and is never overwritten.
    // This empty directory belongs only to this test and is removed explicitly.
    super::mcp_fs::private_directory(&paths.mcp).unwrap();
    assert_status(&paths, InstallationState::Error);
    assert!(
        super::mcp::install_mcp(&paths)
            .await
            .unwrap_err()
            .contains("already exists")
    );
    assert_eq!(fs::read_dir(&paths.mcp).unwrap().count(), 0);
    fs::remove_dir(&paths.mcp).unwrap();
    println!("E2E incomplete status: error; incomplete destination preserved");

    // Fault injection is confined to a completed, attempt-owned staging tree.
    // Activation revalidates it; retention must preserve all staging data.
    let staged = super::mcp::stage_mcp(&paths).await.unwrap();
    let failed_path = staged.path().to_path_buf();
    fs::write(
        staged
            .path()
            .join("node_modules/figma-console-mcp/dist/local.js"),
        b"",
    )
    .unwrap();
    let failure = staged.activate().unwrap_err();
    assert!(failure.contains("durable checkpoint"));
    assert!(failure.contains("staging retained"));
    assert!(
        failed_path.exists(),
        "Modified staging must remain for manual review"
    );
    assert!(!paths.mcp.exists());
    assert_eq!(fs::read_dir(&paths.staging).unwrap().count(), 3);
    assert_eq!(snapshot(&paths.runtime), runtime_before);
    assert_status(&paths, InstallationState::NotInstalled);
    println!(
        "E2E corrupt staging: activation rejected, modified staging retained for review, runtime preserved"
    );

    // Recovery uses the production top-level MCP installer and fresh network
    // resolution frozen to the embedded, reviewed lockfile (scripts disabled).
    super::mcp::install_mcp(&paths).await.unwrap();
    let lock = super::mcp_contract::validate_contract(
        super::mcp_contract::PACKAGE_JSON,
        super::mcp_contract::PACKAGE_LOCK,
    )
    .unwrap();
    super::mcp_fs::validate_installation(&paths.mcp, &lock).unwrap();
    let package = super::mcp_fs::read_regular(
        &paths
            .mcp
            .join("node_modules/figma-console-mcp/package.json"),
        1024 * 1024,
    )
    .unwrap();
    let package: serde_json::Value = serde_json::from_slice(&package).unwrap();
    assert_eq!(package["version"], MCP_VERSION);
    assert_status(&paths, InstallationState::Installed);
    assert_contained(&paths);
    assert_eq!(snapshot(&paths.runtime), runtime_before);
    println!(
        "E2E recovered MCP version: {}; status: installed; all installed paths contained",
        package["version"].as_str().unwrap()
    );

    let mcp_before = snapshot(&paths.mcp);
    let repeat_runtime = super::install::install_node_runtime(&paths, "arm64")
        .await
        .unwrap_err();
    assert!(repeat_runtime.contains("An active runtime already exists"));
    assert!(
        super::mcp::install_mcp(&paths)
            .await
            .unwrap_err()
            .contains("already exists")
    );
    assert_eq!(snapshot(&paths.runtime), runtime_before);
    assert_eq!(snapshot(&paths.mcp), mcp_before);
    assert_eq!(fs::read_dir(&paths.staging).unwrap().count(), 5);
    assert_status(&paths, InstallationState::Installed);
    println!(
        "E2E repeat installation: both refused; content, identities, and permissions unchanged; status: installed"
    );
}
