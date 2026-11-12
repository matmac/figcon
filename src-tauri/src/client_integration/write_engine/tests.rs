use super::*;
use std::{
    fs as disk,
    os::unix::fs::{PermissionsExt, symlink},
};
struct Fixture {
    _temp: tempfile::TempDir,
    engine: Engine,
    target: PathBuf,
}
fn fixture(client: Client, text: Option<&str>) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let home = disk::canonicalize(temp.path()).unwrap();
    disk::set_permissions(&home, disk::Permissions::from_mode(0o700)).unwrap();
    let data = home.join("figcon-data");
    disk::create_dir(&data).unwrap();
    disk::set_permissions(&data, disk::Permissions::from_mode(0o700)).unwrap();
    disk::create_dir(home.join(".codex")).unwrap();
    for (name, bytes) in [
        ("runtime/bin/node", b"\xffsynthetic binary".as_slice()),
        (
            "mcp/node_modules/figma-console-mcp/dist/local.js",
            b"never execute".as_slice(),
        ),
    ] {
        let path = data.join("figcon-installer").join(name);
        disk::create_dir_all(path.parent().unwrap()).unwrap();
        disk::write(path, bytes).unwrap();
    }
    let engine = Engine::isolated(&home, &data).unwrap();
    let target = engine.target(client);
    if let Some(text) = text {
        disk::write(&target, text).unwrap();
        disk::set_permissions(&target, disk::Permissions::from_mode(0o640)).unwrap();
    }
    Fixture {
        _temp: temp,
        engine,
        target,
    }
}
fn baseline(client: Client) -> &'static str {
    match client {
        Client::ClaudeCode => {
            "{\n  \"secret\": \"SYNTHETIC_SECRET\",\n  \"unknown\": {\"keep\": 1},\n  \"mcpServers\": {\"other\": {\"custom\": true}}\n}\n"
        }
        Client::CodexCli => {
            "# header\nsecret = 'SYNTHETIC_SECRET'\n[unknown] # keep\nfield = 1 # inline\n[mcp_servers.other]\ncustom = true # future\n"
        }
    }
}
fn artifacts(f: &Fixture, p: &Plan) -> PathBuf {
    f.engine
        .data
        .join("client-config-transactions")
        .join(&p.binding.id)
}
#[test]
fn creates_missing_files_without_backup() {
    for c in [Client::ClaudeCode, Client::CodexCli] {
        let f = fixture(c, None);
        let p = f.engine.prepare(c).unwrap();
        let id = f.engine.apply(&p).unwrap();
        f.engine.verify_receipt(&id).unwrap();
        assert_eq!(f.engine.recover(&id), Recovery::Verified);
        assert!(!artifacts(&f, &p).join("backup.bin").exists());
        assert_eq!(
            disk::metadata(&f.target).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
#[test]
fn existing_add_preserves_semantics_permissions_and_exact_private_backup() {
    for c in [Client::ClaudeCode, Client::CodexCli] {
        let f = fixture(c, Some(baseline(c)));
        let before = disk::read(&f.target).unwrap();
        let p = f.engine.prepare(c).unwrap();
        let id = f.engine.apply(&p).unwrap();
        f.engine.verify_receipt(&id).unwrap();
        let a = artifacts(&f, &p);
        assert_eq!(disk::read(a.join("backup.bin")).unwrap(), before);
        assert_eq!(
            disk::metadata(a.join("backup.bin"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            disk::metadata(&f.target).unwrap().permissions().mode() & 0o777,
            0o640
        );
        let after = disk::read_to_string(&f.target).unwrap();
        if c == Client::CodexCli {
            for s in [
                "# header",
                "# keep",
                "# inline",
                "# future",
                "secret = 'SYNTHETIC_SECRET'",
            ] {
                assert!(after.contains(s));
            }
        } else {
            assert!(after.starts_with("{\n  \"secret\": \"SYNTHETIC_SECRET\",\n  \"unknown\": {\"keep\": 1},\n  \"mcpServers\": {\"other\": {\"custom\": true}"));
        }
    }
}
#[test]
fn expired_altered_replayed_and_foreign_plans_are_rejected() {
    let f = fixture(Client::ClaudeCode, Some("{}"));
    let p = f
        .engine
        .prepare_for(Client::ClaudeCode, Duration::ZERO)
        .unwrap();
    assert_eq!(f.engine.apply(&p), Err(Code::Expired));
    let p = f.engine.prepare(Client::ClaudeCode).unwrap();
    let mut altered = p.clone();
    altered.binding.target = f.engine.home.join("project.json");
    assert_eq!(f.engine.apply(&altered), Err(Code::Altered));
    let mut altered = p.clone();
    altered.binding.args.push("arbitrary".into());
    assert_eq!(f.engine.apply(&altered), Err(Code::Altered));
    let other = Engine::isolated(&f.engine.home, &f.engine.data).unwrap();
    assert_eq!(other.apply(&p), Err(Code::Altered));
    f.engine.apply(&p).unwrap();
    assert_eq!(f.engine.apply(&p), Err(Code::Consumed));
}
#[test]
fn changed_fingerprint_and_identical_bytes_new_inode_invalidate() {
    for replace_inode in [false, true] {
        let f = fixture(Client::ClaudeCode, Some("{}"));
        let p = f.engine.prepare(Client::ClaudeCode).unwrap();
        if replace_inode {
            let sibling = f.target.with_extension("replacement");
            disk::write(&sibling, "{}").unwrap();
            disk::set_permissions(&sibling, disk::Permissions::from_mode(0o640)).unwrap();
            disk::rename(sibling, &f.target).unwrap();
        } else {
            disk::write(&f.target, "{\"unrelated\":true}").unwrap();
        }
        let changed = disk::read(&f.target).unwrap();
        assert_eq!(f.engine.apply(&p), Err(Code::Changed));
        assert_eq!(disk::read(&f.target).unwrap(), changed);
    }
}
#[test]
fn symlinks_hardlinks_and_unexpected_types_rejected() {
    for kind in [0, 1, 2] {
        let f = fixture(Client::ClaudeCode, None);
        let outside = f.engine.home.join("outside");
        disk::write(&outside, "{}").unwrap();
        match kind {
            0 => symlink(&outside, &f.target).unwrap(),
            1 => disk::hard_link(&outside, &f.target).unwrap(),
            _ => disk::create_dir(&f.target).unwrap(),
        };
        assert!(matches!(
            f.engine.prepare(Client::ClaudeCode),
            Err(Code::Unsafe)
        ));
        assert_eq!(disk::read_to_string(outside).unwrap(), "{}");
    }
}
#[test]
fn malformed_duplicate_unsupported_and_conflicting_documents_preserved() {
    for (client, text, code) in [
        (Client::ClaudeCode, "broken", Code::Malformed),
        (Client::ClaudeCode, r#"{"x":1,"x":2}"#, Code::Malformed),
        (
            Client::ClaudeCode,
            r#"{"mcpServers": []}"#,
            Code::Unsupported,
        ),
        (
            Client::ClaudeCode,
            r#"{"mcpServers":{"figcon-figma-console":{"command":"other"}}}"#,
            Code::Conflict,
        ),
        (Client::CodexCli, "x = ", Code::Malformed),
        (
            Client::CodexCli,
            "[mcp_servers.figcon-figma-console]\ncommand='other'",
            Code::Conflict,
        ),
        (Client::CodexCli, "mcp_servers = {}", Code::Unsupported),
    ] {
        let f = fixture(client, Some(text));
        assert!(matches!(f.engine.prepare(client),Err(e) if e==code));
        assert_eq!(disk::read_to_string(&f.target).unwrap(), text);
    }
}
#[test]
fn permission_and_backup_failures_prevent_mutation() {
    let f = fixture(Client::ClaudeCode, Some("{}"));
    disk::set_permissions(&f.target, disk::Permissions::from_mode(0o400)).unwrap();
    assert!(matches!(
        f.engine.prepare(Client::ClaudeCode),
        Err(Code::Permission)
    ));
    let f = fixture(Client::ClaudeCode, Some("{}"));
    let p = f.engine.prepare(Client::ClaudeCode).unwrap();
    let path = artifacts(&f, &p).join("backup.bin");
    let result = f.engine.apply_with(&p, &mut |point| {
        if matches!(point, Point::BeforeBackup) {
            disk::write(&path, "foreign").unwrap();
        }
        Ok(())
    });
    assert_eq!(result, Err(Code::BackupFailed));
    assert_eq!(disk::read_to_string(&f.target).unwrap(), "{}");
    assert_eq!(disk::read_to_string(path).unwrap(), "foreign");
}
#[test]
fn no_replace_creation_refuses_racing_destination() {
    let f = fixture(Client::ClaudeCode, None);
    let p = f.engine.prepare(Client::ClaudeCode).unwrap();
    let result = f.engine.apply_with(&p, &mut |point| {
        if matches!(point, Point::AfterFinalCheck) {
            disk::write(&f.target, "{\"external\":true}").unwrap();
        }
        Ok(())
    });
    assert_eq!(result, Err(Code::Changed));
    assert_eq!(
        disk::read_to_string(&f.target).unwrap(),
        "{\"external\":true}"
    );
}
#[test]
fn final_reinspection_detects_changes_after_backup() {
    let f = fixture(Client::CodexCli, Some(baseline(Client::CodexCli)));
    let p = f.engine.prepare(Client::CodexCli).unwrap();
    let changed = "# external\nvalue=2\n";
    let result = f.engine.apply_with(&p, &mut |point| {
        if matches!(point, Point::BeforeFinalCheck) {
            disk::write(&f.target, changed).unwrap();
        }
        Ok(())
    });
    assert_eq!(result, Err(Code::Changed));
    assert_eq!(disk::read_to_string(&f.target).unwrap(), changed);
    assert!(!artifacts(&f, &p).join("receipt.json").exists());
}
#[test]
fn post_publish_verification_failure_is_uncertain_and_never_restores_backup() {
    let f = fixture(Client::ClaudeCode, Some("{}"));
    let p = f.engine.prepare(Client::ClaudeCode).unwrap();
    let result = f.engine.apply_with(&p, &mut |point| {
        if matches!(point, Point::AfterPublish) {
            disk::write(&f.target, "{\"external\":true}").unwrap();
        }
        Ok(())
    });
    assert_eq!(result, Err(Code::Uncertain));
    assert_eq!(
        disk::read_to_string(&f.target).unwrap(),
        "{\"external\":true}"
    );
    assert!(artifacts(&f, &p).join("backup.bin").exists());
    assert!(!artifacts(&f, &p).join("receipt.json").exists());
    assert_eq!(f.engine.recover(&p.binding.id), Recovery::NeedsReview);
}
#[test]
fn receipt_failure_keeps_applied_config_and_reports_possible_application() {
    let f = fixture(Client::ClaudeCode, Some("{}"));
    let p = f.engine.prepare(Client::ClaudeCode).unwrap();
    let path = artifacts(&f, &p).join("receipt.json");
    let result = f.engine.apply_with(&p, &mut |point| {
        if matches!(point, Point::BeforeReceipt) {
            disk::write(&path, "unexpected").unwrap();
        }
        Ok(())
    });
    assert_eq!(result, Err(Code::Uncertain));
    assert!(
        disk::read_to_string(&f.target)
            .unwrap()
            .contains(SERVER_NAME)
    );
    assert_eq!(f.engine.recover(&p.binding.id), Recovery::PossiblyApplied);
    assert!(f.engine.verify_receipt(&p.binding.id).is_err());
}
#[test]
fn interrupted_transactions_are_retained_and_recovery_is_idempotent() {
    for after_publish in [false, true] {
        let f = fixture(Client::ClaudeCode, Some("{}"));
        let p = f.engine.prepare(Client::ClaudeCode).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.engine.apply_with(&p, &mut |point| {
                if matches!(point, Point::BeforeFinalCheck) && !after_publish
                    || matches!(point, Point::BeforeReceipt) && after_publish
                {
                    panic!("synthetic interruption");
                }
                Ok(())
            })
        }));
        assert!(result.is_err());
        let expected = if after_publish {
            Recovery::PossiblyApplied
        } else {
            Recovery::NotApplied
        };
        assert_eq!(f.engine.recover(&p.binding.id), expected);
        assert_eq!(f.engine.recover(&p.binding.id), expected);
        assert!(artifacts(&f, &p).join("backup.bin").exists());
        assert!(f.engine.verify_receipt(&p.binding.id).is_err());
    }
}
#[test]
fn stale_receipts_and_incomplete_journals_do_not_authorize_ownership() {
    let f = fixture(Client::CodexCli, Some(baseline(Client::CodexCli)));
    let p = f.engine.prepare(Client::CodexCli).unwrap();
    let id = f.engine.apply(&p).unwrap();
    f.engine.verify_receipt(&id).unwrap();
    let mut text = disk::read_to_string(&f.target).unwrap();
    text.push_str("\n# unrelated update\n");
    disk::write(&f.target, text).unwrap();
    assert_eq!(f.engine.verify_receipt(&id), Err(Code::ReceiptStale));
    let f = fixture(Client::ClaudeCode, None);
    let p = f.engine.prepare(Client::ClaudeCode).unwrap();
    let result = f.engine.apply_with(&p, &mut |point| {
        if matches!(point, Point::AfterReceipt) {
            Err(Code::Io)
        } else {
            Ok(())
        }
    });
    assert_eq!(result, Err(Code::Uncertain));
    assert!(f.engine.verify_receipt(&p.binding.id).is_err());
}
#[test]
fn cooperating_concurrent_engines_cannot_apply_two_stale_plans() {
    let f = fixture(Client::ClaudeCode, Some("{}"));
    let other = Engine::isolated(&f.engine.home, &f.engine.data).unwrap();
    let a = f.engine.prepare(Client::ClaudeCode).unwrap();
    let b = other.prepare(Client::ClaudeCode).unwrap();
    let barrier = std::sync::Barrier::new(2);
    let (one, two) = std::thread::scope(|scope| {
        let one = scope.spawn(|| {
            barrier.wait();
            f.engine.apply(&a)
        });
        let two = scope.spawn(|| {
            barrier.wait();
            other.apply(&b)
        });
        (one.join().unwrap(), two.join().unwrap())
    });
    assert_ne!(one.is_ok(), two.is_ok());
    assert!(
        matches!(
            one.as_ref().err().or(two.as_ref().err()),
            Some(Code::Changed | Code::Busy)
        ),
        "unexpected concurrent results: {one:?} {two:?}"
    );
}
#[test]
fn errors_and_plan_debug_do_not_disclose_configuration_or_paths() {
    let f = fixture(Client::ClaudeCode, Some(baseline(Client::ClaudeCode)));
    let p = f.engine.prepare(Client::ClaudeCode).unwrap();
    let diagnostics = format!("{p:?} {} {:?}", Code::BackupFailed, Code::Uncertain);
    assert!(!diagnostics.contains("SYNTHETIC_SECRET"));
    assert!(!diagnostics.contains(f.engine.home.to_str().unwrap()));
}

#[test]
fn replaced_parent_and_changed_payload_invalidate_approval() {
    let f = fixture(Client::CodexCli, None);
    let p = f.engine.prepare(Client::CodexCli).unwrap();
    disk::rename(
        f.engine.home.join(".codex"),
        f.engine.home.join(".codex-original"),
    )
    .unwrap();
    disk::create_dir(f.engine.home.join(".codex")).unwrap();
    assert_eq!(f.engine.apply(&p), Err(Code::Changed));
    assert!(!f.target.exists());
    let f = fixture(Client::ClaudeCode, Some("{}"));
    let p = f.engine.prepare(Client::ClaudeCode).unwrap();
    disk::write(
        f.engine.data.join("figcon-installer/runtime/bin/node"),
        b"changed",
    )
    .unwrap();
    assert_eq!(f.engine.apply(&p), Err(Code::Changed));
    assert_eq!(disk::read_to_string(&f.target).unwrap(), "{}");
}
#[test]
fn unknown_xattrs_are_rejected_without_losing_metadata() {
    use std::os::fd::AsRawFd;
    let f = fixture(Client::ClaudeCode, Some("{}"));
    let file = disk::File::open(&f.target).unwrap();
    let attribute = c"org.figcon.synthetic";
    assert_eq!(
        unsafe {
            libc::fsetxattr(
                file.as_raw_fd(),
                attribute.as_ptr(),
                b"preserve".as_ptr().cast(),
                8,
                0,
                0,
            )
        },
        0
    );
    assert!(matches!(
        f.engine.prepare(Client::ClaudeCode),
        Err(Code::Unsupported)
    ));
    assert_eq!(disk::read_to_string(&f.target).unwrap(), "{}");
}

// Spawn the Rust test executable, never either AI client. No ambient HOME/config
// variables are inherited; only the parent's private synthetic roots are supplied.
#[test]
fn isolated_lock_holder_child() {
    let Some(home) = std::env::var_os("FIGCON_ENGINE_TEST_HOME") else {
        return;
    };
    let token = std::env::var("FIGCON_ENGINE_TEST_TOKEN").unwrap();
    let home = PathBuf::from(home);
    assert_eq!(
        disk::read_to_string(home.join("test-capability")).unwrap(),
        token
    );
    let engine = Engine::isolated(&home, &home.join("figcon-data")).unwrap();
    let plan = engine.prepare(Client::ClaudeCode).unwrap();
    engine
        .apply_with(&plan, &mut |point| {
            if matches!(point, Point::BeforeFinalCheck) {
                // Publish the completed ID atomically: existence alone must not
                // let the parent observe an empty/partially written readiness file.
                let ready = home.join("child-ready.pending");
                disk::write(&ready, &plan.binding.id).unwrap();
                disk::rename(ready, home.join("child-ready")).unwrap();
                let end = Instant::now() + Duration::from_secs(20);
                while Instant::now() < end {
                    std::thread::park_timeout(Duration::from_millis(10));
                }
                return Err(Code::Io);
            }
            Ok(())
        })
        .unwrap();
}
#[test]
fn cross_process_lock_and_sigkill_recovery_preserve_uncommitted_configuration() {
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let f = fixture(Client::ClaudeCode, Some("{}"));
    let token = nonce().unwrap();
    disk::write(f.engine.home.join("test-capability"), &token).unwrap();
    let mut child = Child(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "client_integration::write_engine::tests::isolated_lock_holder_child",
            ])
            .env_clear()
            .env("FIGCON_ENGINE_TEST_HOME", &f.engine.home)
            .env("FIGCON_ENGINE_TEST_TOKEN", token)
            .current_dir(&f.engine.home)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let end = Instant::now() + Duration::from_secs(10);
    let ready = f.engine.home.join("child-ready");
    while !ready.exists() {
        assert!(Instant::now() < end, "child did not reach commit barrier");
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "child exited before commit barrier"
        );
        std::thread::park_timeout(Duration::from_millis(5));
    }
    let id = disk::read_to_string(ready).unwrap();
    assert_eq!(
        id.len(),
        32,
        "readiness must contain the complete attempt ID"
    );
    let plan = f.engine.prepare(Client::ClaudeCode).unwrap();
    assert_eq!(f.engine.apply(&plan), Err(Code::Busy));
    // Kill only this unreaped owned child PID; no process-group signalling.
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    assert_eq!(disk::read_to_string(&f.target).unwrap(), "{}");
    assert_eq!(f.engine.recover(&id), Recovery::NotApplied);
    assert_eq!(f.engine.recover(&id), Recovery::NotApplied);
    assert!(
        f.engine
            .data
            .join("client-config-transactions")
            .join(id)
            .join("backup.bin")
            .exists()
    );
    let new_plan = f.engine.prepare(Client::ClaudeCode).unwrap();
    f.engine.apply(&new_plan).unwrap();
}

#[test]
fn expiry_at_publication_barrier_does_not_mutate_target() {
    let f = fixture(Client::ClaudeCode, Some("{}"));
    let p = f
        .engine
        .prepare_for(Client::ClaudeCode, Duration::from_secs(2))
        .unwrap();
    let mut reached = false;
    let result = f.engine.apply_with(&p, &mut |point| {
        if matches!(point, Point::AfterFinalCheck) {
            reached = true;
            while Instant::now() < p.expires {
                std::thread::park_timeout(p.expires.saturating_duration_since(Instant::now()));
            }
        }
        Ok(())
    });
    assert!(reached);
    assert_eq!(result, Err(Code::Expired));
    assert_eq!(disk::read_to_string(&f.target).unwrap(), "{}");
    assert!(!artifacts(&f, &p).join("receipt.json").exists());
    let candidate = f
        .target
        .parent()
        .unwrap()
        .join(format!(".figcon-{}.candidate", p.binding.id));
    assert_eq!(
        disk::metadata(candidate).unwrap().permissions().mode() & 0o777,
        0o600
    );
}
