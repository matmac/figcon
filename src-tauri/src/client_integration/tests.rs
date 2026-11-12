use super::*;
use std::fs;
fn context(home: &Path, client_binary: bool) -> Context {
    let binary = home.join("client");
    if client_binary {
        fs::write(&binary, b"must never execute").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
    Context {
        confined: false,
        home: home.into(),
        managed: InstallerPaths::new(home.join("figcon managed")),
        supported: true,
        claude_override: false,
        codex_override: false,
        claude_candidates: vec![binary.clone()],
        codex_candidates: vec![binary],
    }
}
use std::path::Path;
fn write_config(c: &Context, client: Client, text: &str) {
    let path = c.config(client);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn sample(c: &Context, client: Client) -> String {
    let (command, args) = c.launch();
    match client {
        Client::ClaudeCode=>serde_json::json!({"unrelatedSecret":"DO_NOT_EXPOSE", "mcpServers":{SERVER_NAME:{"command":command,"args":args,"type":"stdio"}}}).to_string(),
        Client::CodexCli=>format!("# comment\nunrelated_secret = 'DO_NOT_EXPOSE'\n[mcp_servers.{SERVER_NAME}]\ncommand = {}\nargs = {}\n",serde_json::to_string(&command).unwrap(),serde_json::to_string(&args).unwrap())
    }
}
#[test]
fn parsers_reject_duplicate_keys_and_malformed_documents() {
    for bad in [
        "[]",
        "null",
        "{",
        r#"{"mcpServers":{},"mcpServers":{}}"#,
        r#"{"unknown":{"token":1,"token":2}}"#,
    ] {
        assert!(claude::parse(bad).is_err());
    }
    for bad in [
        "[mcp_servers.a]\ncommand='a'\ncommand='b'",
        "[mcp_servers.a]\n[mcp_servers.a]",
        "not toml",
    ] {
        assert!(codex::parse(bad).is_err());
    }
}
#[test]
fn entry_matching_is_never_ownership_and_unknown_fields_are_conservative() {
    let temp = tempfile::tempdir().unwrap();
    let c = context(temp.path(), false);
    for client in [Client::ClaudeCode, Client::CodexCli] {
        let root = match client {
            Client::ClaudeCode => claude::parse(&sample(&c, client)),
            Client::CodexCli => codex::parse(&sample(&c, client)),
        }
        .unwrap();
        let mut warnings = Vec::new();
        assert_eq!(
            classify(&root, client, &c, &mut warnings),
            RegistrationStatus::MatchingExternal
        );
        assert_eq!(ownership::assess(None, client), OwnershipStatus::Absent);
        let key = if client == Client::ClaudeCode {
            "mcpServers"
        } else {
            "mcp_servers"
        };
        let mut root = root;
        root[key][SERVER_NAME]["env"] = serde_json::json!({"TOKEN":"DO_NOT_EXPOSE"});
        assert_eq!(
            classify(&root, client, &c, &mut warnings),
            RegistrationStatus::Unsupported
        );
    }
}
#[test]
fn ownership_schema_is_not_provenance() {
    use ownership::*;
    let mut record = OwnershipRecord {
        schema_version: 1,
        receipt_id: "synthetic".into(),
        client: Client::ClaudeCode,
        scope: "user".into(),
        server_name: SERVER_NAME.into(),
        configuration_identity: "synthetic".into(),
        entry_sha256: "synthetic".into(),
        managed_installation_identity: "synthetic".into(),
        created_unix_seconds: 1,
        prior_entry_absent: true,
        lifecycle: ReceiptLifecycle::Verified,
    };
    assert_eq!(
        assess(Some(&record), Client::ClaudeCode),
        OwnershipStatus::Unverified
    );
    assert_eq!(
        assess(Some(&record), Client::CodexCli),
        OwnershipStatus::Mismatched
    );
    record.schema_version = 99;
    assert_eq!(
        assess(Some(&record), Client::ClaudeCode),
        OwnershipStatus::Mismatched
    );
}
#[test]
fn unsupported_platform_does_not_inspect_or_initialize_files() {
    let temp = tempfile::tempdir().unwrap();
    let mut c = context(temp.path(), false);
    c.supported = false;
    for client in [Client::ClaudeCode, Client::CodexCli] {
        let report = preview(&c, client);
        assert_eq!(
            report.inspection.configuration,
            ConfigurationStatus::UnsupportedPlatform
        );
        assert!(!report.safe_to_apply);
    }
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}
#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    fn fixture(installed: bool) -> (tempfile::TempDir, Context) {
        let temp = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(temp.path()).unwrap();
        let c = context(&home, installed);
        (temp, c)
    }
    #[test]
    fn preview_requires_both_managed_components_and_never_modifies_them() {
        let (_temp, c) = fixture(true);
        assert!(!preview(&c, Client::ClaudeCode).managed_installation_valid);
        fs::create_dir(&c.managed.root).unwrap();
        fs::set_permissions(&c.managed.root, fs::Permissions::from_mode(0o700)).unwrap();
        for (name, bytes) in [
            ("bin/node", b"must never execute".as_slice()),
            (
                "lib/node_modules/npm/bin/npm-cli.js",
                b"synthetic npm".as_slice(),
            ),
            (
                "lib/node_modules/npm/package.json",
                br#"{"name":"npm","version":"11.19.1"}"#.as_slice(),
            ),
        ] {
            let p = c.managed.runtime.join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, bytes).unwrap();
        }
        fs::set_permissions(
            c.managed.runtime.join("bin/node"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        assert!(!preview(&c, Client::CodexCli).managed_installation_valid);
        let package = include_str!("../../../installer/npm/package.json");
        let lock = include_str!("../../../installer/npm/package-lock.json");
        let parsed: Value = serde_json::from_str(lock).unwrap();
        for (path, entry) in parsed["packages"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(p, _)| !p.is_empty())
        {
            let dir = c.managed.mcp.join(path);
            fs::create_dir_all(&dir).unwrap();
            let mut metadata = entry.clone();
            metadata["name"] = path.rsplit("node_modules/").next().unwrap().into();
            if metadata["name"] == "figma-console-mcp" {
                metadata["bin"]["figma-console-mcp"] = "./dist/local.js".into();
            }
            fs::write(dir.join("package.json"), metadata.to_string()).unwrap();
        }
        fs::write(c.managed.mcp.join("package.json"), package).unwrap();
        fs::write(c.managed.mcp.join("package-lock.json"), lock).unwrap();
        let entry = c
            .managed
            .mcp
            .join("node_modules/figma-console-mcp/dist/local.js");
        fs::create_dir_all(entry.parent().unwrap()).unwrap();
        fs::write(&entry, b"must never execute").unwrap();
        let before = fs::read(&entry).unwrap();
        let report = preview(&c, Client::ClaudeCode);
        assert!(report.managed_installation_valid);
        assert!(!report.safe_to_apply);
        assert_eq!(fs::read(&entry).unwrap(), before);
        assert!(!c.managed.metadata.exists());
        assert!(!c.managed.staging.exists());
        // MCP remains present, but a missing runtime invalidates the proposal.
        fs::remove_file(c.managed.runtime.join("bin/node")).unwrap();
        assert!(!preview(&c, Client::CodexCli).managed_installation_valid);
    }
    #[test]
    fn detects_missing_and_installed_without_execution() {
        for exists in [false, true] {
            let (_temp, c) = fixture(exists);
            for client in [Client::ClaudeCode, Client::CodexCli] {
                let result = inspect(&c, client);
                assert_eq!(
                    result.detection,
                    if exists {
                        DetectionStatus::Detected
                    } else {
                        DetectionStatus::Missing
                    }
                );
                assert_eq!(result.configuration, ConfigurationStatus::Missing);
                assert_eq!(result.registration, RegistrationStatus::NotRegistered);
                assert!(!c.managed.root.exists());
            }
        }
    }
    #[test]
    fn detects_ambiguous_cli_and_deduplicates_symlink_candidates() {
        let (_temp, mut c) = fixture(true);
        let link = c.home.join("alias");
        symlink(c.home.join("client"), &link).unwrap();
        c.claude_candidates.push(link);
        assert_eq!(c.detection(Client::ClaudeCode), DetectionStatus::Detected);
        let other = c.home.join("different");
        fs::write(&other, b"never run").unwrap();
        fs::set_permissions(&other, fs::Permissions::from_mode(0o700)).unwrap();
        c.claude_candidates.push(other);
        assert_eq!(c.detection(Client::ClaudeCode), DetectionStatus::Ambiguous);
    }
    #[test]
    fn matching_external_is_redacted_and_byte_identical_after_every_command() {
        let (_temp, c) = fixture(true);
        for client in [Client::ClaudeCode, Client::CodexCli] {
            let text = sample(&c, client);
            write_config(&c, client, &text);
            let report = inspect(&c, client);
            assert_eq!(report.registration, RegistrationStatus::MatchingExternal);
            assert_eq!(report.ownership, OwnershipStatus::Absent);
            let preview = preview(&c, client);
            assert!(!preview.safe_to_apply);
            assert!(preview.approval_required);
            assert!(!preview.existing_registration_would_be_affected);
            assert!(!preview.managed_installation_valid);
            let json = serde_json::to_string(&preview).unwrap();
            assert!(!json.contains("DO_NOT_EXPOSE"));
            assert!(!json.contains("unrelatedSecret"));
            assert!(Path::new(&preview.proposed_executable).is_absolute());
            assert!(Path::new(&preview.proposed_arguments[0]).is_absolute());
            assert_eq!(fs::read(c.config(client)).unwrap(), text.as_bytes());
            assert!(!c.managed.root.exists());
        }
    }
    #[test]
    fn absent_external_conflicting_and_unsupported_registrations() {
        let (_temp, c) = fixture(true);
        let cases = [
            (r#"{"mcpServers":{}}"#, RegistrationStatus::NotRegistered),
            (
                r#"{"mcpServers":{"other":{"command":"npx","args":["figma-console-mcp@1.46.0"]}}}"#,
                RegistrationStatus::External,
            ),
            (
                r#"{"mcpServers":{"figcon-figma-console":{"command":"external","args":[]}}}"#,
                RegistrationStatus::Conflict,
            ),
            (r#"{"mcpServers":[]}"#, RegistrationStatus::Unsupported),
            (
                r#"{"mcpServers":{"figcon-figma-console":{"command":"external","enabled":false}}}"#,
                RegistrationStatus::Unsupported,
            ),
        ];
        for (text, expected) in cases {
            write_config(&c, Client::ClaudeCode, text);
            assert_eq!(inspect(&c, Client::ClaudeCode).registration, expected);
        }
        for (text, expected) in [
            ("[mcp_servers]", RegistrationStatus::NotRegistered),
            (
                "[mcp_servers.other]\ncommand='npx'\nargs=['figma-console-mcp']",
                RegistrationStatus::External,
            ),
            (
                "[mcp_servers.figcon-figma-console]\ncommand='external'",
                RegistrationStatus::Conflict,
            ),
            (
                "[mcp_servers.figcon-figma-console]\nenabled=false",
                RegistrationStatus::Unsupported,
            ),
        ] {
            write_config(&c, Client::CodexCli, text);
            assert_eq!(inspect(&c, Client::CodexCli).registration, expected);
        }
    }
    #[test]
    fn malformed_inaccessible_symlink_and_hardlink_configs_are_not_read() {
        let (_temp, c) = fixture(true);
        for client in [Client::ClaudeCode, Client::CodexCli] {
            write_config(&c, client, "malformed {");
            assert_eq!(
                inspect(&c, client).configuration,
                ConfigurationStatus::Malformed
            );
            let path = c.config(client);
            fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
            assert_eq!(
                inspect(&c, client).configuration,
                ConfigurationStatus::Inaccessible
            );
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            let outside = c.home.join("secret");
            fs::write(&outside, "DO_NOT_EXPOSE").unwrap();
            fs::remove_file(&path).unwrap();
            symlink(&outside, &path).unwrap();
            assert_eq!(
                inspect(&c, client).configuration,
                ConfigurationStatus::Unsafe
            );
            fs::remove_file(&path).unwrap();
            fs::hard_link(&outside, &path).unwrap();
            assert_eq!(
                inspect(&c, client).configuration,
                ConfigurationStatus::Unsafe
            );
            fs::remove_file(&path).unwrap();
            fs::remove_file(outside).unwrap();
        }
    }
    #[test]
    fn unsafe_ancestor_and_oversized_file_are_rejected() {
        let (_temp, c) = fixture(true);
        let outside = c.home.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("config.toml"), "DO_NOT_EXPOSE").unwrap();
        symlink(&outside, c.home.join(".codex")).unwrap();
        assert_eq!(
            inspect(&c, Client::CodexCli).configuration,
            ConfigurationStatus::Unsafe
        );
        let file = fs::File::create(c.config(Client::ClaudeCode)).unwrap();
        file.set_len(4 * 1024 * 1024 + 1).unwrap();
        assert_eq!(
            inspect(&c, Client::ClaudeCode).configuration,
            ConfigurationStatus::Unsafe
        );
    }
    #[test]
    fn override_locations_are_not_guessed_and_project_shadowing_is_only_a_warning() {
        let (_temp, mut c) = fixture(true);
        c.claude_override = true;
        c.codex_override = true;
        for client in [Client::ClaudeCode, Client::CodexCli] {
            assert_eq!(
                inspect(&c, client).configuration,
                ConfigurationStatus::UnsupportedLocation
            );
        }
        c.claude_override = false;
        write_config(
            &c,
            Client::ClaudeCode,
            r#"{"projects":{"PRIVATE_PROJECT":{"mcpServers":{"figcon-figma-console":{"env":{"TOKEN":"SECRET"}}}}}}"#,
        );
        let report = inspect(&c, Client::ClaudeCode);
        assert!(report.warnings.contains(&Warning::PossibleShadowing));
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("PRIVATE_PROJECT"));
        assert!(!json.contains("SECRET"));
    }
}

#[test]
fn production_environment_preserves_search_locations_and_explicit_injection_has_no_fallback() {
    let home = PathBuf::from("/synthetic/home");
    assert_eq!(
        production_search_roots(&home),
        vec![
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            home.join(".local/bin"),
            home.join(".npm-global/bin")
        ]
    );
    let c = Context::from_roots(
        home.clone(),
        home.join("data"),
        vec![home.join("only-bin")],
        true,
        false,
        false,
    );
    assert_eq!(c.claude_candidates, vec![home.join("only-bin/claude")]);
    assert_eq!(c.codex_candidates, vec![home.join("only-bin/codex")]);
    let c = Context::from_roots(home.clone(), home.join("data"), vec![], true, false, false);
    assert_eq!(c.detection(Client::ClaudeCode), DetectionStatus::Missing);
    assert_eq!(c.detection(Client::CodexCli), DetectionStatus::Missing);
}

#[cfg(all(debug_assertions, target_os = "macos"))]
#[test]
fn native_environment_inspects_synthetic_clients_configs_and_previews_without_execution() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (_temp, session) = crate::acceptance::fixture();
    let roots = session.roots().unwrap().unwrap();
    let c = Context::isolated(roots.clone());
    for client in [Client::ClaudeCode, Client::CodexCli] {
        assert_eq!(inspect(&c, client).detection, DetectionStatus::Missing);
    }
    let trap = roots.home.join("EXECUTED");
    let bytes = format!("#!/bin/sh\ntouch '{}'\nexit 99\n", trap.display());
    for name in ["claude", "codex"] {
        let binary = roots.executables.join(name);
        fs::write(&binary, &bytes).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    }
    for client in [Client::ClaudeCode, Client::CodexCli] {
        assert_eq!(inspect(&c, client).detection, DetectionStatus::Detected);
        assert_eq!(
            inspect(&c, client).configuration,
            ConfigurationStatus::Missing
        );
        write_config(&c, client, &sample(&c, client));
        let result = inspect(&c, client);
        assert_eq!(result.configuration, ConfigurationStatus::Readable);
        assert_eq!(result.registration, RegistrationStatus::MatchingExternal);
        assert!(
            !serde_json::to_string(&result)
                .unwrap()
                .contains("DO_NOT_EXPOSE")
        );
        let preview = preview(&c, client);
        assert!(Path::new(&preview.proposed_executable).starts_with(&roots.data));
        assert!(Path::new(&preview.proposed_arguments[0]).starts_with(&roots.data));
        assert!(!preview.safe_to_apply);
    }
    assert!(!trap.exists());
    fs::rename(
        roots.executables.join("claude"),
        roots.home.join("outside-client"),
    )
    .unwrap();
    symlink(
        roots.home.join("outside-client"),
        roots.executables.join("claude"),
    )
    .unwrap();
    assert_eq!(
        inspect(&c, Client::ClaudeCode).detection,
        DetectionStatus::Missing
    );
    fs::remove_file(roots.executables.join("claude")).unwrap();
    fs::hard_link(
        roots.home.join("outside-client"),
        roots.executables.join("claude"),
    )
    .unwrap();
    assert_eq!(
        inspect(&c, Client::ClaudeCode).detection,
        DetectionStatus::Missing
    );
}
