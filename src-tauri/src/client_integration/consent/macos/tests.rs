use super::*;
use crate::installer::paths::InstallerPaths;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
struct Fixture {
    _temp: tempfile::TempDir,
    context: Context,
}
fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(temp.path()).unwrap();
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).unwrap();
    let data = home.join("data");
    fs::create_dir(&data).unwrap();
    fs::set_permissions(&data, fs::Permissions::from_mode(0o700)).unwrap();
    fs::create_dir(home.join(".codex")).unwrap();
    let binary = home.join("client");
    fs::write(&binary, "never execute").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let paths = InstallerPaths::new(data.join("figcon-installer"));
    fs::create_dir(&paths.root).unwrap();
    fs::set_permissions(&paths.root, fs::Permissions::from_mode(0o700)).unwrap();
    for (name, text) in [
        ("bin/node", "never execute"),
        ("lib/node_modules/npm/bin/npm-cli.js", "never execute"),
        (
            "lib/node_modules/npm/package.json",
            r#"{"name":"npm","version":"11.19.1"}"#,
        ),
    ] {
        let p = paths.runtime.join(name);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    fs::set_permissions(
        paths.runtime.join("bin/node"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let manifest = include_str!("../../../../../installer/npm/package.json");
    let lock = include_str!("../../../../../installer/npm/package-lock.json");
    let parsed: serde_json::Value = serde_json::from_str(lock).unwrap();
    for (path, entry) in parsed["packages"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(p, _)| !p.is_empty())
    {
        let dir = paths.mcp.join(path);
        fs::create_dir_all(&dir).unwrap();
        let mut package = entry.clone();
        package["name"] = path.rsplit("node_modules/").next().unwrap().into();
        if package["name"] == "figma-console-mcp" {
            package["bin"]["figma-console-mcp"] = "./dist/local.js".into();
        }
        fs::write(dir.join("package.json"), package.to_string()).unwrap();
    }
    fs::write(paths.mcp.join("package.json"), manifest).unwrap();
    fs::write(paths.mcp.join("package-lock.json"), lock).unwrap();
    let entry = paths
        .mcp
        .join("node_modules/figma-console-mcp/dist/local.js");
    fs::create_dir_all(entry.parent().unwrap()).unwrap();
    fs::write(entry, "never execute").unwrap();
    Fixture {
        _temp: temp,
        context: Context {
            confined: false,
            home,
            managed: paths,
            supported: true,
            claude_override: false,
            codex_override: false,
            claude_candidates: vec![binary.clone()],
            codex_candidates: vec![binary],
        },
    }
}
fn code<T>(result: Result<T, ConsentError>) -> ConsentCode {
    match result {
        Ok(_) => panic!("Expected a closed error"),
        Err(e) => e.code,
    }
}
fn storage(f: &Fixture) -> PathBuf {
    f.context
        .managed
        .root
        .parent()
        .unwrap()
        .join("client-config-transactions")
}
fn private(path: &std::path::Path) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
#[test]
fn preview_is_plan_derived_and_disabled_approval_consumes_without_writes() {
    let f = fixture();
    fs::write(
        f.context.config(Client::ClaudeCode),
        r#"{"private":"SYNTHETIC_SECRET"}"#,
    )
    .unwrap();
    let before = fs::read(f.context.config(Client::ClaudeCode)).unwrap();
    let s = Service::default();
    let p = s
        .prepare(f.context.clone(), Client::ClaudeCode, "main")
        .unwrap();
    assert_eq!(p.plan_id.len(), 32);
    assert!(!p.production_mutation_enabled);
    assert_eq!(p.scope, "user");
    assert!(p.expires_in_seconds <= 300);
    assert!(p.proposed_executable.ends_with("runtime/bin/node"));
    assert!(p.proposed_arguments[0].ends_with("figma-console-mcp/dist/local.js"));
    assert!(
        !serde_json::to_string(&p)
            .unwrap()
            .contains("SYNTHETIC_SECRET")
    );
    assert_eq!(
        code(s.approve(f.context.clone(), &p.plan_id, "main")),
        ConsentCode::ActivationDisabled
    );
    assert_eq!(
        code(s.approve(f.context.clone(), &p.plan_id, "main")),
        ConsentCode::ConsumedPlan
    );
    assert_eq!(
        fs::read(f.context.config(Client::ClaudeCode)).unwrap(),
        before
    );
    assert!(!storage(&f).exists());
}
#[test]
fn plans_are_window_owned_reject_tampering_and_expire_without_sleep() {
    let f = fixture();
    let s = Service::default();
    let p = s
        .prepare(f.context.clone(), Client::CodexCli, "one")
        .unwrap();
    assert_eq!(
        code(s.approve(f.context.clone(), &p.plan_id, "two")),
        ConsentCode::NotOwner
    );
    assert_eq!(
        code(s.approve(f.context.clone(), "../config", "one")),
        ConsentCode::UnknownPlan
    );
    s.inner
        .lock()
        .unwrap()
        .entries
        .get_mut(&p.plan_id)
        .unwrap()
        .expires = Instant::now();
    assert_eq!(
        code(s.approve(f.context.clone(), &p.plan_id, "one")),
        ConsentCode::ExpiredPlan
    );
    s.prune();
    assert!(s.inner.lock().unwrap().entries.is_empty());
    assert!(!storage(&f).exists());
}
#[test]
fn declined_plan_cannot_be_approved() {
    let f = fixture();
    let s = Service::default();
    let p = s
        .prepare(f.context.clone(), Client::ClaudeCode, "one")
        .unwrap();
    assert_eq!(s.reject(&p.plan_id, "one").unwrap().state, "rejected");
    assert_eq!(
        code(s.approve(f.context.clone(), &p.plan_id, "one")),
        ConsentCode::Rejected
    );
    assert!(!f.context.config(Client::ClaudeCode).exists());
}
#[test]
fn stale_configuration_and_incorrect_targets_fail_closed() {
    let f = fixture();
    let s = Service::default();
    let p = s
        .prepare(f.context.clone(), Client::ClaudeCode, "one")
        .unwrap();
    fs::write(
        f.context.config(Client::ClaudeCode),
        "{\"newSetting\":true}",
    )
    .unwrap();
    assert_eq!(
        code(s.approve(f.context.clone(), &p.plan_id, "one")),
        ConsentCode::StalePlan
    );
    let p = s
        .prepare(f.context.clone(), Client::ClaudeCode, "one")
        .unwrap();
    let mut context = f.context.clone();
    context.home = context.home.join("different");
    assert_eq!(
        code(s.approve(context, &p.plan_id, "one")),
        ConsentCode::StalePlan
    );
    assert!(!storage(&f).exists());
}
#[test]
fn missing_prerequisites_client_and_unsupported_profile_are_rejected() {
    let f = fixture();
    let s = Service::default();
    let mut context = f.context.clone();
    context.claude_candidates.clear();
    assert_eq!(
        code(s.prepare(context, Client::ClaudeCode, "main")),
        ConsentCode::ClientUnavailable
    );
    let mut context = f.context.clone();
    context.codex_override = true;
    assert_eq!(
        code(s.prepare(context, Client::CodexCli, "main")),
        ConsentCode::UnsupportedConfiguration
    );
    fs::remove_file(f.context.managed.runtime.join("bin/node")).unwrap();
    assert_eq!(
        code(s.prepare(f.context.clone(), Client::ClaudeCode, "main")),
        ConsentCode::PrerequisitesMissing
    );
    assert!(serde_json::from_str::<Client>("\"arbitrary-command\"").is_err());
    assert!(!storage(&f).exists());
}
#[test]
fn registry_capacity_is_bounded_and_expired_entries_release_slots() {
    let f = fixture();
    let s = Service::default();
    for _ in 0..MAX_PLANS {
        s.prepare(f.context.clone(), Client::ClaudeCode, "main")
            .unwrap();
    }
    assert_eq!(
        code(s.prepare(f.context.clone(), Client::ClaudeCode, "main")),
        ConsentCode::RegistryCapacity
    );
    {
        let mut r = s.inner.lock().unwrap();
        assert!(r.entries.values().map(|e| e.bytes).sum::<usize>() <= MAX_REGISTRY_BYTES);
        for e in r.entries.values_mut() {
            e.expires = Instant::now();
        }
    }
    s.prune();
    s.prepare(f.context.clone(), Client::ClaudeCode, "main")
        .unwrap();
    assert!(!storage(&f).exists());
}
#[test]
fn recovery_counts_and_bytes_block_exhaustion_without_cleanup() {
    let f = fixture();
    let root = storage(&f);
    fs::create_dir(&root).unwrap();
    private(&root);
    for n in 0..32 {
        let dir = root.join(format!("{n:032x}"));
        fs::create_dir(&dir).unwrap();
        private(&dir);
    }
    let engine =
        Engine::preview_only(&f.context.home, f.context.managed.root.parent().unwrap()).unwrap();
    let usage = engine.recovery_usage();
    assert_eq!(usage.artifact_count, Some(32));
    assert_eq!(usage.storage_bytes, Some(0));
    assert_eq!(usage.state, RecoveryState::Full);
    assert!(!usage.automatic_cleanup_enabled);
    let s = Service::default();
    assert_eq!(
        code(s.prepare(f.context.clone(), Client::ClaudeCode, "main")),
        ConsentCode::RecoveryCapacity
    );
    assert_eq!(fs::read_dir(&root).unwrap().count(), 32);
    let f = fixture();
    let root = storage(&f);
    fs::create_dir(&root).unwrap();
    private(&root);
    let dir = root.join("retained");
    fs::create_dir(&dir).unwrap();
    private(&dir);
    let file = fs::File::create(dir.join("backup.bin")).unwrap();
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .unwrap();
    file.set_len(64 * 1024 * 1024).unwrap();
    let usage = Engine::preview_only(&f.context.home, f.context.managed.root.parent().unwrap())
        .unwrap()
        .recovery_usage();
    assert_eq!(usage.storage_bytes, Some(64 * 1024 * 1024));
    assert_eq!(usage.state, RecoveryState::Full);
    assert!(dir.join("backup.bin").exists());
}
#[test]
fn unsafe_orphan_and_symlink_artifacts_report_unknown_without_following() {
    use std::os::unix::fs::symlink;
    let f = fixture();
    let root = storage(&f);
    fs::create_dir(&root).unwrap();
    private(&root);
    let outside = f.context.home.join("outside");
    fs::write(&outside, "SYNTHETIC_SECRET").unwrap();
    symlink(&outside, root.join("unowned")).unwrap();
    let usage = Engine::preview_only(&f.context.home, f.context.managed.root.parent().unwrap())
        .unwrap()
        .recovery_usage();
    assert_eq!(usage.state, RecoveryState::Unknown);
    assert!(
        !serde_json::to_string(&usage)
            .unwrap()
            .contains("SYNTHETIC_SECRET")
    );
    assert_eq!(fs::read_to_string(outside).unwrap(), "SYNTHETIC_SECRET");
}
#[test]
fn synthetic_approval_is_single_use_and_engine_gate_cannot_be_bypassed() {
    let f = fixture();
    let s = Service::default();
    let p = s
        .prepare_inner(f.context.clone(), Client::CodexCli, "main", true)
        .unwrap();
    let receipt = s
        .approve_inner(f.context.clone(), &p.plan_id, "main", true)
        .unwrap();
    assert_eq!(receipt.state, "verified");
    assert_eq!(
        code(s.approve_inner(f.context.clone(), &p.plan_id, "main", true)),
        ConsentCode::ConsumedPlan
    );
    let f = fixture();
    let s = Service::default();
    let p = s
        .prepare(f.context.clone(), Client::ClaudeCode, "main")
        .unwrap();
    assert_eq!(
        code(s.approve_inner(f.context.clone(), &p.plan_id, "main", true)),
        ConsentCode::ActivationDisabled
    );
    assert!(!storage(&f).exists());
}
#[test]
fn duplicate_approval_calls_do_not_publish_twice() {
    let f = fixture();
    let s = Service::default();
    let p = s
        .prepare_inner(f.context.clone(), Client::ClaudeCode, "main", true)
        .unwrap();
    let gate = std::sync::Barrier::new(2);
    let (a, b) = std::thread::scope(|scope| {
        let one = scope.spawn(|| {
            gate.wait();
            s.approve_inner(f.context.clone(), &p.plan_id, "main", true)
        });
        let two = scope.spawn(|| {
            gate.wait();
            s.approve_inner(f.context.clone(), &p.plan_id, "main", true)
        });
        (one.join().unwrap(), two.join().unwrap())
    });
    assert_ne!(a.is_ok(), b.is_ok());
    let error = a.err().or(b.err()).unwrap();
    assert_eq!(error.code, ConsentCode::ConsumedPlan);
    assert_eq!(
        fs::read_dir(storage(&f))
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_type().unwrap().is_dir())
            .count(),
        1
    );
}

#[test]
fn receipt_persistence_failure_is_uncertain_and_never_restores_backup() {
    let f = fixture();
    fs::write(f.context.config(Client::ClaudeCode), "{}").unwrap();
    let s = Service::default();
    let p = s
        .prepare_inner(f.context.clone(), Client::ClaudeCode, "main", true)
        .unwrap();
    let root = storage(&f);
    fs::create_dir(&root).unwrap();
    private(&root);
    let transaction = root.join(&p.plan_id);
    fs::create_dir(&transaction).unwrap();
    private(&transaction);
    let receipt = transaction.join("receipt.json");
    fs::write(&receipt, "unexpected retained file").unwrap();
    fs::set_permissions(&receipt, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        code(s.approve_inner(f.context.clone(), &p.plan_id, "main", true)),
        ConsentCode::Uncertain
    );
    assert!(
        fs::read_to_string(f.context.config(Client::ClaudeCode))
            .unwrap()
            .contains("figcon-figma-console")
    );
    assert_eq!(
        fs::read_to_string(transaction.join("backup.bin")).unwrap(),
        "{}"
    );
    assert_eq!(
        fs::read_to_string(receipt).unwrap(),
        "unexpected retained file"
    );
}
#[test]
fn late_capacity_exhaustion_rejects_approval_and_retains_every_artifact() {
    let f = fixture();
    let s = Service::default();
    let p = s
        .prepare_inner(f.context.clone(), Client::CodexCli, "main", true)
        .unwrap();
    let root = storage(&f);
    fs::create_dir(&root).unwrap();
    private(&root);
    for n in 0..32 {
        let dir = root.join(format!("{n:032x}"));
        fs::create_dir(&dir).unwrap();
        private(&dir);
    }
    assert_eq!(
        code(s.approve_inner(f.context.clone(), &p.plan_id, "main", true)),
        ConsentCode::RecoveryCapacity
    );
    assert!(!f.context.config(Client::CodexCli).exists());
    assert_eq!(fs::read_dir(root).unwrap().count(), 32);
}

#[cfg(debug_assertions)]
#[test]
fn native_acceptance_consent_binds_isolated_targets_and_keeps_production_gate_closed() {
    let source = fixture();
    let (_temp, session) = crate::acceptance::fixture();
    let roots = session.roots().unwrap().unwrap();
    fs::rename(
        &source.context.managed.root,
        roots.data.join("figcon-installer"),
    )
    .unwrap();
    for name in ["claude", "codex"] {
        let p = roots.executables.join(name);
        fs::write(&p, "never execute").unwrap();
        fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let context = Context::isolated(roots.clone());
    let service = Service::default();
    for client in [Client::ClaudeCode, Client::CodexCli] {
        let path = context.config(client);
        let original = if client == Client::ClaudeCode {
            "{}"
        } else {
            "# private synthetic settings\n"
        };
        fs::write(&path, original).unwrap();
        let plan = service
            .prepare(context.clone(), client, "native-window")
            .unwrap();
        assert!(PathBuf::from(&plan.proposed_executable).starts_with(&roots.data));
        assert!(PathBuf::from(&plan.proposed_arguments[0]).starts_with(&roots.data));
        assert_eq!(
            service
                .approve(context.clone(), &plan.plan_id, "native-window")
                .unwrap_err()
                .code,
            ConsentCode::ActivationDisabled
        );
        assert_eq!(fs::read(&path).unwrap(), original.as_bytes());
        let plan = service
            .prepare(context.clone(), client, "native-window")
            .unwrap();
        fs::write(
            &path,
            if client == Client::ClaudeCode {
                "{\"synthetic\":1}"
            } else {
                "synthetic=1\n"
            },
        )
        .unwrap();
        assert_eq!(
            service
                .approve(context.clone(), &plan.plan_id, "native-window")
                .unwrap_err()
                .code,
            ConsentCode::StalePlan
        );
    }
}
