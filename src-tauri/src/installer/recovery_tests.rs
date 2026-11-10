use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::attempt::{InstallationKind, Lifecycle};
use super::cleanup::StagingAttempt;
use super::paths::InstallerPaths;
use super::recovery::{RetentionReason, recover_installations};

#[test]
#[ignore = "child-process fixture invoked only by recovery tests"]
fn durable_attempt_fixture() {
    let root = PathBuf::from(
        std::env::var_os("FIGCON_RECOVERY_FIXTURE_ROOT").expect("Fixture root required"),
    );
    let mode = std::env::var("FIGCON_RECOVERY_FIXTURE_MODE").unwrap();
    let paths = InstallerPaths::new(root.clone());
    if mode == "admission" {
        let gate_root = root.parent().unwrap();
        fs::write(
            gate_root.join(format!("gate-ready-{}", std::process::id())),
            b"ready",
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !gate_root.join("admit-now").exists() {
            assert!(
                Instant::now() < deadline,
                "Admission fixture gate timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        match StagingAttempt::create(&paths, InstallationKind::Mcp) {
            Ok(attempt) => {
                drop(attempt);
                std::process::exit(0);
            }
            Err(error) if error.contains("Too many retained attempts") => std::process::exit(42),
            Err(_) => panic!("Unexpected admission fixture failure"),
        }
    }
    let attempt = StagingAttempt::create(&paths, InstallationKind::Mcp).unwrap();
    fs::write(attempt.path().join("owned-payload"), b"owned").unwrap();
    attempt.checkpoint(Lifecycle::Prepared).unwrap();
    if mode == "running" {
        attempt.state(Lifecycle::Running).unwrap();
    }
    let ready = root.join(format!("ready-{}", std::process::id()));
    let temporary = ready.with_extension("tmp");
    fs::write(
        &temporary,
        attempt.path().file_name().unwrap().to_str().unwrap(),
    )
    .unwrap();
    fs::rename(temporary, ready).unwrap();
    if mode == "exit" {
        std::process::exit(0);
    }
    loop {
        std::thread::park_timeout(Duration::from_secs(1));
    }
}

fn start(root: &Path, mode: &str) -> (Child, PathBuf) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "installer::recovery_tests::durable_attempt_fixture",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("FIGCON_RECOVERY_FIXTURE_ROOT", root)
        .env("FIGCON_RECOVERY_FIXTURE_MODE", mode)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let ready = root.join(format!("ready-{}", child.id()));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "Attempt fixture exited before creating its journal"
        );
        assert!(
            Instant::now() < deadline,
            "Attempt fixture did not initialize"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let name = fs::read_to_string(ready).unwrap();
    (child, root.join("staging").join(name))
}

fn ledger(paths: &InstallerPaths, directory: &Path) -> PathBuf {
    paths.metadata.join("attempts").join(format!(
        "{}.json",
        directory.file_name().unwrap().to_str().unwrap()
    ))
}

#[test]
fn retains_crashed_checkpoint_idempotently_and_preserves_active_installs() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallerPaths::new(temp.path().join("figcon"));
    let (mut child, directory) = start(&paths.root, "exit");
    assert!(child.wait().unwrap().success());
    for active in [&paths.runtime, &paths.mcp] {
        fs::create_dir(active).unwrap();
        fs::write(active.join("marker"), b"active").unwrap();
    }
    let report = recover_installations(&paths).unwrap();
    assert!(report.removed.is_empty());
    assert_eq!(report.retained[0].reason, RetentionReason::ManualReview);
    assert!(directory.exists());
    assert!(ledger(&paths, &directory).exists());
    assert!(recover_installations(&paths).unwrap().removed.is_empty());
    for active in [&paths.runtime, &paths.mcp] {
        assert_eq!(fs::read(active.join("marker")).unwrap(), b"active");
    }
}

#[test]
fn live_and_concurrent_attempts_remain_locked_until_their_owner_exits() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallerPaths::new(temp.path().join("figcon"));
    let (mut one, first) = start(&paths.root, "live");
    let (mut two, second) = start(&paths.root, "live");
    assert_ne!(first, second);
    let report = recover_installations(&paths).unwrap();
    assert!(report.removed.is_empty());
    assert_eq!(report.retained.len(), 2);
    assert!(
        report
            .retained
            .iter()
            .all(|entry| entry.reason == RetentionReason::Live)
    );
    one.kill().unwrap();
    one.wait().unwrap();
    assert!(recover_installations(&paths).unwrap().removed.is_empty());
    assert!(second.exists());
    two.kill().unwrap();
    two.wait().unwrap();
    assert!(recover_installations(&paths).unwrap().removed.is_empty());
}

#[test]
fn modified_corrupt_unknown_and_symlinked_directories_are_never_adopted() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallerPaths::new(temp.path().join("figcon"));
    let (mut child, directory) = start(&paths.root, "exit");
    child.wait().unwrap();
    fs::write(directory.join("foreign-file"), b"external").unwrap();
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("marker"), b"outside").unwrap();
    std::os::unix::fs::symlink(&outside, paths.staging.join("unknown-link")).unwrap();
    fs::create_dir(paths.staging.join("unknown-directory")).unwrap();
    for _ in 0..2 {
        let report = recover_installations(&paths).unwrap();
        assert!(report.removed.is_empty());
        assert!(
            report
                .retained
                .iter()
                .any(|entry| entry.reason == RetentionReason::Modified)
        );
        assert_eq!(
            fs::read(directory.join("foreign-file")).unwrap(),
            b"external"
        );
        assert_eq!(fs::read(outside.join("marker")).unwrap(), b"outside");
    }
    fs::write(ledger(&paths, &directory), b"corrupt metadata").unwrap();
    assert!(recover_installations(&paths).unwrap().removed.is_empty());
    assert!(directory.exists());
}

#[test]
fn interrupted_running_attempt_and_reused_pid_require_manual_review() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallerPaths::new(temp.path().join("figcon"));
    let (mut child, directory) = start(&paths.root, "running");
    child.kill().unwrap();
    child.wait().unwrap();
    let report = recover_installations(&paths).unwrap();
    assert_eq!(report.retained[0].reason, RetentionReason::Unsealed);
    assert!(directory.exists());
    let record_path = ledger(&paths, &directory);
    let mut record: super::attempt::Record =
        serde_json::from_slice(&fs::read(&record_path).unwrap()).unwrap();
    record.process.pid = std::process::id();
    record.process.start_seconds = 1;
    fs::write(&record_path, serde_json::to_vec(&record).unwrap()).unwrap();
    assert_eq!(
        recover_installations(&paths).unwrap().retained[0].reason,
        RetentionReason::UnknownLiveness
    );
}

#[tokio::test]
async fn running_quota_violation_terminates_owned_process_and_retains_unsealed_data() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallerPaths::new(temp.path().join("figcon"));
    let attempt = StagingAttempt::create(&paths, InstallationKind::Mcp).unwrap();
    attempt.state(Lifecycle::Running).unwrap();
    fs::create_dir_all(attempt.path().join("npm-environment/cache")).unwrap();
    let mut command = tokio::process::Command::new("/bin/sh");
    command.current_dir(attempt.path()).env_clear().args([
        "-c",
        "echo excessive > npm-environment/cache/data; /bin/sleep 0.5; echo escaped > survived",
    ]);
    let limits = super::limits::Limits {
        cache_bytes: 1,
        interval: Duration::from_millis(10),
        ..super::limits::MCP
    };
    let result = super::limits::monitor(
        &attempt,
        limits,
        super::process::run(&mut command, Duration::from_secs(5), false),
    )
    .await;
    assert!(result.unwrap_err().contains("npm-cache quota"));
    tokio::time::sleep(Duration::from_millis(750)).await;
    assert!(!attempt.path().join("survived").exists());
    let path = attempt.path().to_path_buf();
    assert_eq!(
        attempt.cleanup().unwrap(),
        super::cleanup::CleanupDisposition::RetainedForReview
    );
    assert!(path.join("npm-environment/cache/data").exists());
}

#[test]
fn metadata_replacement_and_directory_replacement_prevent_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallerPaths::new(temp.path().join("figcon"));
    let (mut child, directory) = start(&paths.root, "exit");
    child.wait().unwrap();
    fs::rename(&directory, paths.staging.join("moved-original")).unwrap();
    fs::create_dir(&directory).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(directory.join("marker"), b"replacement").unwrap();
    assert!(recover_installations(&paths).unwrap().removed.is_empty());
    assert_eq!(fs::read(directory.join("marker")).unwrap(), b"replacement");
}

#[test]
fn symlinked_receipt_is_preserved_without_touching_its_target() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallerPaths::new(temp.path().join("figcon"));
    let (mut child, directory) = start(&paths.root, "exit");
    child.wait().unwrap();
    let receipt = ledger(&paths, &directory);
    let outside = temp.path().join("outside-record");
    fs::rename(&receipt, &outside).unwrap();
    let original = fs::read(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, &receipt).unwrap();
    let report = recover_installations(&paths).unwrap();
    assert!(report.removed.is_empty());
    assert_eq!(report.retained[0].reason, RetentionReason::UnknownOwnership);
    assert_eq!(fs::read(outside).unwrap(), original);
    assert!(directory.exists());
}

#[tokio::test]
async fn npm_child_inherits_a_hard_single_file_limit() {
    let temp = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new("/bin/dd");
    command.current_dir(temp.path()).env_clear().args([
        "if=/dev/zero",
        "of=large",
        "bs=1048576",
        "count=65",
    ]);
    super::limits::configure(&mut command);
    assert!(
        super::process::run(&mut command, Duration::from_secs(10), false)
            .await
            .is_err()
    );
    assert!(fs::metadata(temp.path().join("large")).unwrap().len() <= 64 * 1024 * 1024);
}

#[test]
fn orphaned_ledger_files_are_reported_repeatedly_without_removal() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallerPaths::new(temp.path().join("figcon"));
    let attempt = StagingAttempt::create(&paths, InstallationKind::Mcp).unwrap();
    attempt.cleanup().unwrap();
    // Only this fixture explicitly removes its own temporary tree to simulate
    // an orphan ledger; production cleanup never performs this operation.
    fs::remove_dir_all(&paths.staging).unwrap();
    let orphan = paths.metadata.join("attempts/unknown.tmp");
    fs::write(&orphan, b"preserve").unwrap();
    for _ in 0..2 {
        assert_eq!(
            recover_installations(&paths).unwrap().orphaned_ledger_files,
            3
        );
        assert_eq!(fs::read(&orphan).unwrap(), b"preserve");
    }
}
#[test]
fn concurrent_admission_allows_only_eight_attempts() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallerPaths::new(temp.path().join("figcon"));
    super::root::prepare_installation_root(&paths.root).unwrap();
    let barrier = std::sync::Barrier::new(12);
    let attempts = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..12)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    StagingAttempt::create(&paths, InstallationKind::Mcp)
                })
            })
            .collect();
        threads
            .into_iter()
            .filter_map(|t| t.join().unwrap().ok())
            .collect::<Vec<_>>()
    });
    assert_eq!(attempts.len(), 8);
    assert_eq!(fs::read_dir(&paths.staging).unwrap().count(), 8);
    for attempt in attempts {
        attempt.cleanup().unwrap();
    }
}
#[test]
fn expired_validation_and_checkpoint_preserve_staging() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallerPaths::new(temp.path().join("figcon"));
    let attempt = StagingAttempt::create(&paths, InstallationKind::Mcp).unwrap();
    let expires = super::deadline::Deadline::new(Duration::from_millis(5));
    let _scope = expires.enter();
    std::thread::sleep(Duration::from_millis(10));
    assert!(
        super::mcp_fs::validate_tree(attempt.path())
            .unwrap_err()
            .contains("deadline")
    );
    assert!(
        attempt
            .checkpoint(Lifecycle::Prepared)
            .unwrap_err()
            .contains("deadline")
    );
    assert!(attempt.path().exists());
}
#[test]
fn insertion_into_abandoned_attempt_is_retained_by_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallerPaths::new(temp.path().join("figcon"));
    let (mut child, directory) = start(&paths.root, "exit");
    child.wait().unwrap();
    fs::write(directory.join("unexpected"), b"keep").unwrap();
    for _ in 0..2 {
        assert!(recover_installations(&paths).unwrap().removed.is_empty());
        assert_eq!(fs::read(directory.join("unexpected")).unwrap(), b"keep");
    }
}

#[test]
fn deadline_expires_during_checkpoint_hashing_without_publishing_a_new_record() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallerPaths::new(temp.path().join("figcon"));
    let attempt = StagingAttempt::create(&paths, InstallationKind::Mcp).unwrap();
    let receipt = ledger(&paths, attempt.path());
    let original = fs::read(&receipt).unwrap();
    fs::File::create(attempt.path().join("large"))
        .unwrap()
        .set_len(64 * 1024 * 1024)
        .unwrap();
    let deadline = super::deadline::Deadline::new(Duration::from_millis(5));
    let _scope = deadline.enter();
    assert!(
        attempt
            .checkpoint(Lifecycle::Prepared)
            .unwrap_err()
            .contains("deadline")
    );
    assert_eq!(fs::read(receipt).unwrap(), original);
    assert!(attempt.path().join("large").exists());
}

#[test]
fn deadline_expiring_inside_validation_is_checked_before_success() {
    let temp = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("unused", temp.path().join("link")).unwrap();
    let deadline = super::deadline::Deadline::new(Duration::from_millis(5));
    let _scope = deadline.enter();
    let result = super::mcp_fs::validate_tree_with_links(temp.path(), |_| {
        std::thread::sleep(Duration::from_millis(10));
        Ok(())
    });
    assert!(result.unwrap_err().contains("deadline"));
}

#[test]
fn multi_process_admission_serializes_twelve_contenders_to_eight_attempts() {
    struct Children(Vec<Child>);
    impl Drop for Children {
        fn drop(&mut self) {
            for child in &mut self.0 {
                if matches!(child.try_wait(), Ok(None)) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                }
            }
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallerPaths::new(temp.path().join("figcon"));
    super::root::prepare_installation_root(&paths.root).unwrap();
    let mut children = Children(Vec::new());
    for _ in 0..12 {
        children.0.push(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "installer::recovery_tests::durable_attempt_fixture",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("FIGCON_RECOVERY_FIXTURE_ROOT", &paths.root)
                .env("FIGCON_RECOVERY_FIXTURE_MODE", "admission")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    while !children
        .0
        .iter()
        .all(|c| temp.path().join(format!("gate-ready-{}", c.id())).exists())
    {
        assert!(
            Instant::now() < deadline,
            "Admission contenders did not reach gate"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !paths.staging.exists(),
        "No contender may allocate before the shared gate"
    );
    fs::write(temp.path().join("admit-now"), b"release").unwrap();
    let mut results = Vec::new();
    for child in &mut children.0 {
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                results.push(status.code());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Admission contender did not complete"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    assert_eq!(results.iter().filter(|code| **code == Some(0)).count(), 8);
    assert_eq!(results.iter().filter(|code| **code == Some(42)).count(), 4);
    assert_eq!(fs::read_dir(&paths.staging).unwrap().count(), 8);
    let receipts = fs::read_dir(paths.metadata.join("attempts"))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|ext| ext == "json")
        })
        .count();
    assert_eq!(receipts, 8);
    assert!(recover_installations(&paths).unwrap().removed.is_empty());
}
