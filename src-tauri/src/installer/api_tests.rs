use super::{api_models::*, api_service::Service, progress::Sink};
use std::sync::{Arc, Mutex};
fn observer() -> (Sink, Arc<Mutex<Vec<InstallationEvent>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let output = events.clone();
    (
        Arc::new(move |event| output.lock().unwrap().push(event)),
        events,
    )
}
#[test]
fn lifecycle_ordering_duplicate_and_cross_component_admission() {
    let service = Service::default();
    let (sink, events) = observer();
    let a = service
        .reserve(Component::Node, "main".into(), "a".into(), sink.clone())
        .unwrap();
    for component in [Component::Node, Component::Mcp] {
        assert_eq!(
            service
                .reserve(component, "main".into(), "other".into(), sink.clone())
                .err()
                .unwrap()
                .code,
            ErrorCode::Busy
        );
    }
    a.progress.staging();
    a.progress.milestone(Stage::Downloading);
    a.progress.download(57, Some(100), true);
    a.progress.milestone(Stage::Verifying);
    a.progress.milestone(Stage::Extracting);
    a.progress.milestone(Stage::Validating);
    a.progress.milestone(Stage::Activating);
    a.progress.activated();
    service.finish(&a, Ok(()));
    a.progress.milestone(Stage::Downloading); // Late callbacks cannot pollute terminal history.
    let b = service
        .reserve(Component::Mcp, "main".into(), "b".into(), sink)
        .unwrap();
    service.finish(&b, Err("PRIVATE_PATH SECRET_TOKEN".into()));
    let list = events.lock().unwrap();
    let aa: Vec<_> = list.iter().filter(|e| e.attempt_id == "a").collect();
    assert_eq!(aa.first().unwrap().stage, Stage::Preparing);
    assert_eq!(aa.last().unwrap().stage, Stage::Completed);
    assert!(aa.last().unwrap().retained_staging && aa.last().unwrap().activated);
    assert!(
        aa.iter()
            .enumerate()
            .all(|(i, e)| e.sequence == (i + 1) as u64 && e.component == Component::Node)
    );
    assert!(
        aa.iter()
            .any(|e| e.downloaded_bytes == Some(57) && e.total_bytes == Some(100))
    );
    let bb: Vec<_> = list.iter().filter(|e| e.attempt_id == "b").collect();
    assert_eq!(bb[0].sequence, 1);
    assert_eq!(bb.last().unwrap().stage, Stage::Failed);
    assert!(
        !serde_json::to_string(&*list)
            .unwrap()
            .contains("SECRET_TOKEN")
    );
}
#[tokio::test]
async fn cancellation_is_attempt_owned_cooperative_and_retained() {
    let service = Service::default();
    let (sink, events) = observer();
    let op = service
        .reserve(Component::Mcp, "main".into(), "cancel".into(), sink)
        .unwrap();
    assert_eq!(
        service.cancel("main", "wrong").unwrap_err().code,
        ErrorCode::NotActive
    );
    assert_eq!(
        service.cancel("other", "cancel").unwrap_err().code,
        ErrorCode::NotOwner
    );
    let (tx, rx) = tokio::sync::oneshot::channel();
    let deadline = op.deadline.clone();
    let task = tokio::spawn(async move {
        super::deadline::run_with(deadline, move || async move {
            super::progress::staging_created();
            let _ = tx.send(());
            loop {
                super::deadline::check()?;
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            #[allow(unreachable_code)]
            Ok::<(), String>(())
        })
        .await
    });
    rx.await.unwrap();
    assert_eq!(
        service.cancel("main", "cancel").unwrap().disposition,
        CancellationDisposition::Requested
    );
    assert_eq!(
        service.cancel("main", "cancel").unwrap().disposition,
        CancellationDisposition::AlreadyRequested
    );
    service.finish(&op, task.await.unwrap());
    let events = events.lock().unwrap();
    let terminal = events.last().unwrap();
    assert_eq!(terminal.stage, Stage::Cancelled);
    assert!(terminal.retained_staging);
    assert!(!terminal.activated);
    assert_eq!(
        service.cancel("main", "cancel").unwrap_err().code,
        ErrorCode::NotActive
    );
}
#[test]
fn closed_error_mapping_and_attempt_limit() {
    use super::api_service::map_error;
    for (raw, expected) in [
        ("archive checksum mismatch", ErrorCode::IntegrityFailed),
        ("API runtime required", ErrorCode::RuntimeRequired),
        ("API destination exists", ErrorCode::AlreadyInstalled),
        (
            "Installation deadline exceeded",
            ErrorCode::DeadlineExceeded,
        ),
        ("quota violation", ErrorCode::ResourceLimit),
        ("failed npm execution", ErrorCode::OperationFailed),
    ] {
        assert_eq!(map_error(raw), expected);
    }
    let service = Service::default();
    let (sink, events) = observer();
    let op = service
        .reserve(Component::Node, "main".into(), "limit".into(), sink)
        .unwrap();
    op.progress.hint(ErrorCode::AttemptLimit);
    service.finish(&op, Err("private raw error".into()));
    assert_eq!(
        events
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .code,
        ErrorCode::AttemptLimit
    );
}
#[test]
fn wire_contract_uses_camel_case_and_kebab_enums() {
    let event = InstallationEvent {
        revision: 1,
        schema_version: 1,
        attempt_id: "fixture".into(),
        component: Component::Node,
        sequence: 1,
        stage: Stage::Downloading,
        status: EventStatus::Running,
        downloaded_bytes: Some(42),
        total_bytes: None,
        retained_staging: true,
        activated: false,
        error: Some(InstallationError::new(ErrorCode::DeadlineExceeded)),
    };
    let value = serde_json::to_value(&event).unwrap();
    assert_eq!(value["attemptId"], "fixture");
    assert_eq!(value["downloadedBytes"], 42);
    assert_eq!(value["error"]["code"], "deadline-exceeded");
    assert_eq!(
        serde_json::from_value::<InstallationEvent>(value).unwrap(),
        event
    );
}
#[cfg(target_os = "macos")]
#[test]
fn committed_activation_rejects_cancellation_and_preserves_success() {
    let service = Service::default();
    let (sink, events) = observer();
    let op = service
        .reserve(Component::Node, "main".into(), "committed".into(), sink)
        .unwrap();
    let _scope = op.deadline.enter();
    super::deadline::activate(|| Ok(())).unwrap();
    assert_eq!(
        service.cancel("main", "committed").unwrap().disposition,
        CancellationDisposition::TooLate
    );
    service.finish(&op, Ok(()));
    assert_eq!(
        events.lock().unwrap().last().unwrap().stage,
        Stage::Completed
    );
}
#[cfg(target_os = "macos")]
#[test]
fn cancellation_before_activation_prevents_filesystem_mutation() {
    let service = Service::default();
    let (sink, _) = observer();
    let op = service
        .reserve(Component::Node, "main".into(), "precommit".into(), sink)
        .unwrap();
    service.cancel("main", "precommit").unwrap();
    let _scope = op.deadline.enter();
    assert!(super::deadline::activate(|| panic!("Cancelled activation executed")).is_err());
}
#[tokio::test]
async fn disk_status_is_read_only_and_separate_from_cancelled_operation() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = super::paths::InstallerPaths::new(tmp.path().join("managed"));
    let mut report = super::api_backend::inspect(&paths);
    assert!(!paths.root.exists());
    assert_eq!(report.node.disk_state, DiskState::NotInstalled);
    let service = Service::default();
    let (sink, _) = observer();
    let op = service
        .reserve(Component::Node, "main".into(), "status".into(), sink)
        .unwrap();
    service.snapshot(&mut report).unwrap();
    assert_eq!(report.node.state, OperationState::Installing);
    service.cancel("main", "status").unwrap();
    service.finish(&op, Err("cancel".into()));
    let mut report = super::api_backend::inspect(&paths);
    service.snapshot(&mut report).unwrap();
    assert_eq!(report.node.state, OperationState::Cancelled);
    assert_eq!(report.node.disk_state, DiskState::NotInstalled);
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use crate::installer::{api_backend, deadline, paths::InstallerPaths};
    #[tokio::test]
    async fn actual_mcp_pipeline_events_status_receipt_and_repeat_protection() {
        let temp = tempfile::tempdir().unwrap();
        let paths = crate::installer::mcp::tests::macos::setup(temp.path(), "success");
        let service = Service::default();
        let (sink, events) = observer();
        let id = crate::installer::attempt::new_id().unwrap();
        let op = service
            .reserve(Component::Mcp, "main".into(), id.clone(), sink)
            .unwrap();
        service.execute(op, paths.clone()).await;
        let events = events.lock().unwrap();
        assert_eq!(events.last().unwrap().stage, Stage::Completed);
        let stages: Vec<_> = events.iter().map(|e| e.stage).collect();
        let installing = stages.iter().position(|s| *s == Stage::Installing).unwrap();
        let activating = stages.iter().position(|s| *s == Stage::Activating).unwrap();
        assert!(
            installing < activating
                && stages[installing + 1..activating].contains(&Stage::Validating)
        );
        assert!(
            events
                .iter()
                .all(|e| e.attempt_id == id && e.downloaded_bytes.is_none())
        );
        drop(events);
        let ledger = std::fs::read_dir(paths.metadata.join("attempts"))
            .unwrap()
            .filter_map(Result::ok)
            .find(|e| e.path().extension().is_some_and(|v| v == "json"))
            .unwrap();
        let record = crate::installer::attempt::load(&ledger.path()).unwrap();
        assert_eq!(record.attempt_id, id);
        let inspected = paths.clone();
        let mut report = deadline::run(move || async move { Ok(api_backend::inspect(&inspected)) })
            .await
            .unwrap();
        service.snapshot(&mut report).unwrap();
        assert_eq!(report.mcp.disk_state, DiskState::Installed);
        assert_eq!(report.retention.state, RetentionState::RequiresReview);
        assert_eq!(report.retention.attempt_count, Some(1));
        let (sink, repeat) = observer();
        let op = service
            .reserve(Component::Mcp, "main".into(), "repeat".into(), sink)
            .unwrap();
        service.execute(op, paths.clone()).await;
        assert_eq!(
            repeat
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .code,
            ErrorCode::AlreadyInstalled
        );
        assert_eq!(std::fs::read_dir(paths.staging).unwrap().count(), 1);
    }
    #[tokio::test]
    async fn real_npm_failure_and_explicit_cancel_never_activate_and_keep_staging() {
        for action in ["failed", "interrupted"] {
            let temp = tempfile::tempdir().unwrap();
            let paths = crate::installer::mcp::tests::macos::setup(temp.path(), action);
            let service = Arc::new(Service::default());
            let (sink, events) = observer();
            let op = service
                .reserve(
                    Component::Mcp,
                    "main".into(),
                    crate::installer::attempt::new_id().unwrap(),
                    sink,
                )
                .unwrap();
            let id = op.progress.id.clone();
            let runner = service.clone();
            let target = paths.clone();
            let task = tokio::spawn(async move { runner.execute(op, target).await });
            if action == "interrupted" {
                tokio::time::timeout(std::time::Duration::from_secs(10), async {
                    loop {
                        let ready = std::fs::read_dir(&paths.staging).ok().is_some_and(|list| {
                            list.filter_map(Result::ok)
                                .any(|e| e.path().join("mcp/ready").exists())
                        });
                        if ready {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    }
                })
                .await
                .unwrap();
                assert_eq!(
                    service.cancel("main", &id).unwrap().disposition,
                    CancellationDisposition::Requested
                );
            }
            task.await.unwrap();
            assert!(!paths.mcp.exists());
            let list = events.lock().unwrap();
            let terminal = list.last().unwrap();
            assert!(terminal.retained_staging && !terminal.activated);
            assert_eq!(
                terminal.stage,
                if action == "failed" {
                    Stage::Failed
                } else {
                    Stage::Cancelled
                }
            );
            assert!(
                !serde_json::to_string(&*list)
                    .unwrap()
                    .contains("SYNTHETIC_SECRET")
            );
            drop(list);
            tokio::time::sleep(std::time::Duration::from_millis(700)).await;
            for entry in std::fs::read_dir(&paths.staging).unwrap() {
                assert!(!entry.unwrap().path().join("mcp/child-survived").exists());
            }
        }
    }
    #[tokio::test]
    async fn missing_runtime_is_typed_and_no_attempt_is_allocated() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("managed"));
        let service = Service::default();
        let (sink, events) = observer();
        let op = service
            .reserve(Component::Mcp, "main".into(), "missing".into(), sink)
            .unwrap();
        service.execute(op, paths.clone()).await;
        let list = events.lock().unwrap();
        assert_eq!(
            list.last().unwrap().error.as_ref().unwrap().code,
            ErrorCode::RuntimeRequired
        );
        assert!(!list.last().unwrap().retained_staging);
        assert!(!paths.staging.exists());
    }
    #[tokio::test]
    async fn timeout_during_cooperative_worker_is_failed_not_cancelled() {
        let service = Service::default();
        let (sink, events) = observer();
        let op = service
            .reserve(Component::Node, "main".into(), "timeout".into(), sink)
            .unwrap();
        let progress = op.progress.clone();
        let d = deadline::Deadline::observed(std::time::Duration::from_millis(10), Some(progress));
        let result = deadline::run_with(d, move || async move {
            deadline::run(move || async move {
                super::super::progress::staging_created();
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                Ok(())
            })
            .await
        })
        .await;
        service.finish(&op, result);
        let events = events.lock().unwrap();
        let event = events.last().unwrap();
        assert_eq!(event.stage, Stage::Failed);
        assert_eq!(
            event.error.as_ref().unwrap().code,
            ErrorCode::DeadlineExceeded
        );
    }
    #[test]
    #[ignore = "subprocess fixture, invoked by operation_lock_serializes_processes"]
    fn operation_lock_child() {
        let Some(root) = std::env::var_os("FIGCON_API_LOCK_TEST_ROOT") else {
            return;
        };
        let paths = InstallerPaths::new(root.into());
        let locked = api_backend::operation_lock(&paths);
        if std::env::var_os("FIGCON_API_LOCK_EXPECT_BUSY").is_some() {
            assert_eq!(locked.unwrap_err(), "API operation busy");
        } else {
            assert!(locked.is_ok());
        }
    }
    #[test]
    fn operation_lock_serializes_processes() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("managed"));
        let lock = api_backend::operation_lock(&paths).unwrap();
        let child = |busy: bool| {
            let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
            cmd.args([
                "--exact",
                "installer::api_tests::macos::operation_lock_child",
                "--ignored",
            ])
            .env("FIGCON_API_LOCK_TEST_ROOT", &paths.root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
            if busy {
                cmd.env("FIGCON_API_LOCK_EXPECT_BUSY", "1");
            }
            assert!(cmd.status().unwrap().success());
        };
        child(true);
        drop(lock);
        child(false);
    }
}

#[test]
fn racing_component_requests_admit_exactly_one_operation() {
    let service = Arc::new(Service::default());
    let gate = Arc::new(std::sync::Barrier::new(12));
    let threads: Vec<_> = (0..12)
        .map(|i| {
            let service = service.clone();
            let gate = gate.clone();
            std::thread::spawn(move || {
                gate.wait();
                service
                    .reserve(
                        if i % 2 == 0 {
                            Component::Node
                        } else {
                            Component::Mcp
                        },
                        "main".into(),
                        format!("{i}"),
                        Arc::new(|_| {}),
                    )
                    .is_ok()
            })
        })
        .collect();
    assert_eq!(
        threads
            .into_iter()
            .map(|t| usize::from(t.join().unwrap()))
            .sum::<usize>(),
        1
    );
}
#[tokio::test]
async fn worker_observers_do_not_leak_between_attempts() {
    let (first, aa) = observer();
    let (second, bb) = observer();
    let run = |id: &str, sink: Sink| {
        let p = Arc::new(super::progress::Progress::new(
            id.into(),
            Component::Node,
            sink,
        ));
        let deadline =
            super::deadline::Deadline::observed(std::time::Duration::from_secs(5), Some(p));
        super::deadline::run_with(deadline, move || async move {
            super::progress::milestone(Stage::Verifying);
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            super::progress::milestone(Stage::Extracting);
            Ok(())
        })
    };
    let (a, b) = tokio::join!(run("first", first), run("second", second));
    a.unwrap();
    b.unwrap();
    assert_eq!(aa.lock().unwrap().len(), 2);
    assert_eq!(bb.lock().unwrap().len(), 2);
    assert!(aa.lock().unwrap().iter().all(|e| e.attempt_id == "first"));
    assert!(bb.lock().unwrap().iter().all(|e| e.attempt_id == "second"));
}
#[cfg(not(target_os = "macos"))]
#[tokio::test]
async fn unsupported_backend_has_no_filesystem_side_effects() {
    let temp = tempfile::tempdir().unwrap();
    let paths = super::paths::InstallerPaths::new(temp.path().join("managed"));
    assert!(
        super::api_backend::execute(paths.clone(), Component::Node)
            .await
            .is_err()
    );
    assert!(!paths.root.exists());
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn actual_attempt_limit_is_blocked_without_downloading_or_allocating() {
    use std::os::unix::fs::DirBuilderExt;
    let temp = tempfile::tempdir().unwrap();
    let paths = super::paths::InstallerPaths::new(temp.path().join("managed"));
    super::root::prepare_installation_root(&paths.root).unwrap();
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&paths.staging)
        .unwrap();
    for i in 0..8 {
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(paths.staging.join(format!("unknown-{i}")))
            .unwrap();
    }
    let service = Service::default();
    let (sink, events) = observer();
    let op = service
        .reserve(
            Component::Node,
            "main".into(),
            super::attempt::new_id().unwrap(),
            sink,
        )
        .unwrap();
    service.execute(op, paths.clone()).await;
    let list = events.lock().unwrap();
    let terminal = list.last().unwrap();
    assert_eq!(
        terminal.error.as_ref().unwrap().code,
        ErrorCode::AttemptLimit
    );
    assert!(!terminal.retained_staging);
    assert!(!list.iter().any(|e| e.stage == Stage::Downloading));
    drop(list);
    assert_eq!(std::fs::read_dir(&paths.staging).unwrap().count(), 8);
    let mut report = super::api_backend::inspect(&paths);
    service.snapshot(&mut report).unwrap();
    assert_eq!(report.node.state, OperationState::Blocked);
    assert_eq!(report.retention.attempt_count, Some(8));
    assert_eq!(
        report.node.last_operation.unwrap().state,
        OperationState::Blocked
    );
}
#[cfg(unix)]
#[test]
fn status_rejects_symlink_and_insecure_managed_roots_without_repair() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let paths = super::paths::InstallerPaths::new(temp.path().join("managed"));
    std::fs::create_dir(&paths.root).unwrap();
    std::fs::set_permissions(&paths.root, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        super::api_backend::inspect(&paths).node.disk_state,
        DiskState::Invalid
    );
    assert_eq!(
        std::fs::metadata(&paths.root).unwrap().permissions().mode() & 0o777,
        0o755
    );
    let linked = super::paths::InstallerPaths::new(temp.path().join("linked"));
    std::os::unix::fs::symlink(&paths.root, &linked.root).unwrap();
    assert_eq!(
        super::api_backend::inspect(&linked).node.disk_state,
        DiskState::Invalid
    );
}

#[tokio::test]
async fn unstable_snapshot_retries_are_bounded_and_typed() {
    let temp = tempfile::tempdir().unwrap();
    let paths = super::paths::InstallerPaths::new(temp.path().join("managed"));
    let service = Service::default();
    let (sink, _) = observer();
    let op = service
        .reserve(Component::Node, "main".into(), "revision".into(), sink)
        .unwrap();
    let mut reads = 0;
    let result = service
        .inspect(|| {
            reads += 1;
            let report = super::api_backend::inspect(&paths);
            op.progress.milestone(Stage::Validating);
            async { Ok(report) }
        })
        .await;
    assert_eq!(reads, 3);
    assert_eq!(result.unwrap_err().code, ErrorCode::UnstableSnapshot);
    let before = service.revision().unwrap();
    service.cancel("main", "revision").unwrap();
    assert!(service.revision().unwrap() > before);
    let report = service
        .inspect(|| async { Ok(super::api_backend::inspect(&paths)) })
        .await
        .unwrap();
    assert!(report.node.last_operation.unwrap().cancellation_requested);
}
#[test]
fn synchronous_listener_can_read_current_state_and_publish_reentrantly() {
    let temp = tempfile::tempdir().unwrap();
    let paths = super::paths::InstallerPaths::new(temp.path().join("managed"));
    let service = Arc::new(Service::default());
    let weak = Arc::downgrade(&service);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let output = seen.clone();
    let sink = Arc::new(move |event: InstallationEvent| {
        let service = weak.upgrade().unwrap();
        let report = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(service.inspect(|| async { Ok(super::api_backend::inspect(&paths)) }))
            .unwrap();
        assert!(report.revision >= event.revision);
        output.lock().unwrap().push(event);
    });
    let op = service
        .reserve(Component::Node, "main".into(), "listener".into(), sink)
        .unwrap();
    op.progress.milestone(Stage::Validating);
    service.finish(&op, Err("fixture failure".into()));
    let events = seen.lock().unwrap();
    assert_eq!(events.len(), 3);
    assert!(
        events
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence && pair[0].revision < pair[1].revision)
    );
}
#[test]
fn reentrant_dispatch_keeps_sequence_without_holding_progress_lock() {
    let holder = Arc::new(Mutex::new(
        None::<std::sync::Weak<super::progress::Progress>>,
    ));
    let nested = holder.clone();
    let events = Arc::new(Mutex::new(Vec::new()));
    let output = events.clone();
    let p = Arc::new(super::progress::Progress::new(
        "reentrant".into(),
        Component::Node,
        Arc::new(move |event| {
            output.lock().unwrap().push(event.sequence);
            if event.sequence == 1 {
                let p = nested.lock().unwrap().as_ref().unwrap().upgrade().unwrap();
                p.milestone(Stage::Validating);
            }
        }),
    ));
    *holder.lock().unwrap() = Some(Arc::downgrade(&p));
    p.milestone(Stage::Preparing);
    assert_eq!(*events.lock().unwrap(), vec![1, 2]);
}
#[cfg(target_os = "macos")]
#[tokio::test]
async fn completion_while_disk_inspection_paused_retries_actual_installation() {
    let temp = tempfile::tempdir().unwrap();
    let paths = super::mcp::tests::macos::setup(temp.path(), "success");
    let service = Arc::new(Service::default());
    let (sink, _) = observer();
    let op = service
        .reserve(
            Component::Mcp,
            "main".into(),
            super::attempt::new_id().unwrap(),
            sink,
        )
        .unwrap();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let mut ready = Some(ready_tx);
    let mut release = Some(release_rx);
    let target = paths.clone();
    let observer = service.clone();
    let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = reads.clone();
    let query = tokio::spawn(async move {
        observer
            .inspect(move || {
                count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let report = super::api_backend::inspect(&target);
                let ready = ready.take();
                let release = release.take();
                async move {
                    if let Some(ready) = ready {
                        ready.send(()).unwrap();
                    }
                    if let Some(release) = release {
                        release.await.unwrap();
                    }
                    Ok(report)
                }
            })
            .await
    });
    ready_rx.await.unwrap();
    service.execute(op, paths.clone()).await;
    release_tx.send(()).unwrap();
    let report = query.await.unwrap().unwrap();
    assert_eq!(reads.load(std::sync::atomic::Ordering::Relaxed), 2);
    assert_eq!(report.mcp.disk_state, DiskState::Installed);
    assert_eq!(
        report.mcp.last_operation.unwrap().state,
        OperationState::Installed
    );
    assert_eq!(report.revision, service.revision().unwrap());
}
