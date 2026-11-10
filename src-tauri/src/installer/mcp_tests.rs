use super::*;

#[cfg(not(target_os = "macos"))]
#[tokio::test]
async fn unsupported_platform_has_no_side_effects() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallerPaths::new(temp.path().join("figcon"));
    assert!(stage_mcp(&paths).await.is_err());
    assert!(!paths.root.exists());
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn fixture(root: &Path) {
        let lock = validate_contract(PACKAGE_JSON, PACKAGE_LOCK).unwrap();
        for (path, entry) in lock["packages"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(p, _)| !p.is_empty())
        {
            fs::create_dir_all(root.join(path)).unwrap();
            let name = path.rsplit("node_modules/").next().unwrap();
            let mut metadata = entry.clone();
            metadata["name"] = name.into();
            if name == "figma-console-mcp" {
                metadata["bin"]["figma-console-mcp"] = "./dist/local.js".into();
            }
            fs::write(root.join(path).join("package.json"), metadata.to_string()).unwrap();
        }
        fs::create_dir_all(root.join("node_modules/figma-console-mcp/dist")).unwrap();
        fs::write(
            root.join("node_modules/figma-console-mcp/dist/local.js"),
            b"throw new Error('must never execute during install');",
        )
        .unwrap();
    }

    fn shell_quote(path: &Path) -> String {
        format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
    }

    fn setup(base: &Path, action: &str) -> InstallerPaths {
        let paths = InstallerPaths::new(base.join("figcon"));
        super::super::super::root::prepare_installation_root(&paths.root).unwrap();
        super::super::super::test_support::write_runtime(&paths.runtime);
        let data = base.join("fixture");
        fixture(&data);
        let ci = match action {
            "failed" => "echo SYNTHETIC_SECRET_DO_NOT_REPORT >&2; exit 23".to_string(),
            "interrupted" => {
                "echo ready > ready; ( /bin/sleep 0.5; echo leaked > child-survived ) & wait".into()
            }
            _ => format!("/bin/cp -R {}/node_modules .", shell_quote(&data)),
        };
        let ls = if action == "invalid-tree" {
            "exit 24"
        } else {
            "exit 0"
        };
        let script = format!(
            r#"#!/bin/sh
if [ "$1" = --version ]; then echo v24.22.2; exit 0; fi
if [ "$2" = --version ]; then echo 11.0.0; exit 0; fi
case " $* " in *" --ignore-scripts "*) ;; *) exit 40 ;; esac
case " $* " in *" --no-bin-links "*) ;; *) exit 41 ;; esac
[ -z "$NODE_OPTIONS" ] && [ -z "$NODE_PATH" ] && [ -z "$FIGMA_ACCESS_TOKEN" ] || exit 42
[ "$HOME" != "" ] && [ -d "$npm_config_cache" ] && [ -f "$npm_config_userconfig" ] && [ -f "$npm_config_globalconfig" ] || exit 43
case "$2" in
ci) {ci} ;;
ls) {ls} ;;
*) exit 44 ;;
esac
"#
        );
        fs::write(paths.runtime.join("bin/node"), script).unwrap();
        fs::set_permissions(
            paths.runtime.join("bin/node"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        paths
    }

    #[tokio::test]
    async fn expired_staged_handle_cannot_activate() {
        let temp = tempfile::tempdir().unwrap();
        let paths = setup(temp.path(), "success");
        let mut staged = stage_mcp(&paths).await.unwrap();
        let stage = staged.path().to_path_buf();
        staged.attempt.as_mut().unwrap().deadline =
            super::super::super::deadline::Deadline::new(std::time::Duration::ZERO);
        assert!(
            staged
                .activate_async()
                .await
                .unwrap_err()
                .contains("deadline")
        );
        assert!(!paths.mcp.exists());
        assert!(stage.exists());
    }

    #[tokio::test]
    async fn stages_and_activates_without_executing_mcp() {
        let temp = tempfile::tempdir().unwrap();
        let paths = setup(temp.path(), "success");
        let runtime = fs::read(paths.runtime.join("bin/node")).unwrap();
        let staged = stage_mcp(&paths).await.unwrap();
        assert!(!paths.mcp.exists());
        assert_eq!(
            fs::read(staged.path().join("package-lock.json")).unwrap(),
            PACKAGE_LOCK.as_bytes()
        );
        assert!(!staged.path().join("node_modules/.bin").exists());
        let discarded = stage_mcp(&paths).await.unwrap();
        discarded.discard().unwrap();
        assert!(staged.path().is_dir());
        assert_eq!(fs::read_dir(&paths.staging).unwrap().count(), 2);
        staged.activate().unwrap();
        assert_eq!(
            super::super::super::status::inspect_installation(&paths).state,
            super::super::super::models::InstallationState::Installed
        );
        assert!(
            paths
                .mcp
                .join("node_modules/figma-console-mcp/dist/local.js")
                .is_file()
        );
        assert!(!paths.mcp.join("npm-environment").exists());
        assert_eq!(fs::read_dir(&paths.staging).unwrap().count(), 2);
        assert_eq!(fs::read(paths.runtime.join("bin/node")).unwrap(), runtime);
    }

    #[tokio::test]
    async fn missing_entry_point_and_dependency_mismatch_are_rejected() {
        for missing_entry in [true, false] {
            let temp = tempfile::tempdir().unwrap();
            let paths = setup(temp.path(), "success");
            let package = temp.path().join("fixture/node_modules/figma-console-mcp");
            if missing_entry {
                fs::remove_file(package.join("dist/local.js")).unwrap();
            } else {
                fs::write(
                    package.join("package.json"),
                    br#"{"name":"figma-console-mcp","version":"0.0.0"}"#,
                )
                .unwrap();
            }
            assert!(stage_mcp(&paths).await.is_err());
            assert!(!paths.mcp.exists());
            assert_eq!(fs::read_dir(&paths.staging).unwrap().count(), 1);
        }
    }

    #[tokio::test]
    async fn failed_npm_and_invalid_tree_are_retained_without_exposing_output() {
        for action in ["failed", "invalid-tree"] {
            let temp = tempfile::tempdir().unwrap();
            let paths = setup(temp.path(), action);
            let error = stage_mcp(&paths).await.err().unwrap();
            assert!(error.contains("Installer process failed"));
            assert!(!error.contains("SYNTHETIC_SECRET"));
            assert!(!error.contains(temp.path().to_str().unwrap()));
            assert!(!paths.mcp.exists());
            assert_eq!(fs::read_dir(&paths.staging).unwrap().count(), 1);
        }
    }

    #[tokio::test]
    async fn existing_destination_is_preserved_before_staging_and_at_activation() {
        let temp = tempfile::tempdir().unwrap();
        let paths = setup(temp.path(), "success");
        let staged = stage_mcp(&paths).await.unwrap();
        fs::create_dir(&paths.mcp).unwrap();
        fs::write(paths.mcp.join("marker"), b"existing").unwrap();
        assert!(staged.activate().is_err());
        assert!(stage_mcp(&paths).await.is_err());
        assert_eq!(fs::read(paths.mcp.join("marker")).unwrap(), b"existing");
        assert_eq!(fs::read_dir(&paths.staging).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn concurrent_activations_cannot_replace_each_other() {
        let temp = tempfile::tempdir().unwrap();
        let paths = setup(temp.path(), "success");
        let (one, two) = tokio::join!(stage_mcp(&paths), stage_mcp(&paths));
        let (one, two) = (one.unwrap(), two.unwrap());
        fs::write(
            one.path()
                .join("node_modules/figma-console-mcp/dist/winner"),
            b"one",
        )
        .unwrap();
        fs::write(
            two.path()
                .join("node_modules/figma-console-mcp/dist/winner"),
            b"two",
        )
        .unwrap();
        one.attempt
            .as_ref()
            .unwrap()
            .checkpoint(super::super::super::attempt::Lifecycle::Prepared)
            .unwrap();
        two.attempt
            .as_ref()
            .unwrap()
            .checkpoint(super::super::super::attempt::Lifecycle::Prepared)
            .unwrap();
        let barrier = std::sync::Barrier::new(2);
        let (one, two) = std::thread::scope(|scope| {
            let a = scope.spawn(|| {
                barrier.wait();
                one.activate()
            });
            let b = scope.spawn(|| {
                barrier.wait();
                two.activate()
            });
            (a.join().unwrap(), b.join().unwrap())
        });
        assert_ne!(one.is_ok(), two.is_ok());
        assert_eq!(
            fs::read(paths.mcp.join("node_modules/figma-console-mcp/dist/winner")).unwrap(),
            if one.is_ok() { b"one" } else { b"two" }
        );
        assert_eq!(fs::read_dir(&paths.staging).unwrap().count(), 2);
    }

    #[tokio::test]
    async fn interruption_kills_process_group_and_retains_owned_staging() {
        let temp = tempfile::tempdir().unwrap();
        let paths = setup(temp.path(), "interrupted");
        let mut installation = Box::pin(stage_mcp(&paths));
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        let staged = loop {
            assert!(
                tokio::time::Instant::now() < deadline,
                "npm fixture did not start"
            );
            tokio::select! {
                result = &mut installation => panic!("Unexpected completion: {}", result.is_ok()),
                _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {}
            }
            let Ok(entries) = fs::read_dir(&paths.staging) else {
                continue;
            };
            let ready = entries
                .map(|e| e.unwrap().path().join("mcp"))
                .find(|p| p.join("ready").exists());
            if let Some(path) = ready {
                break path;
            }
        };
        drop(installation);
        tokio::time::sleep(std::time::Duration::from_millis(750)).await;
        assert!(staged.exists());
        assert!(!staged.join("child-survived").exists());
        assert!(!paths.mcp.exists());
    }

    #[test]
    fn installed_tree_rejects_symlink_escapes() {
        let temp = tempfile::tempdir().unwrap();
        let tree = temp.path().join("tree");
        fs::create_dir(&tree).unwrap();
        std::os::unix::fs::symlink(temp.path(), tree.join("escape")).unwrap();
        assert!(super::super::super::mcp_fs::validate_tree(&tree).is_err());
    }

    #[tokio::test]
    async fn activation_rejects_mutated_graph_and_reports_cleanup_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let paths = setup(temp.path(), "success");
        let staged = stage_mcp(&paths).await.unwrap();
        let metadata = staged
            .path()
            .join("node_modules/figma-console-mcp/package.json");
        let mut package: Value = serde_json::from_slice(&fs::read(&metadata).unwrap()).unwrap();
        package["dependencies"] = serde_json::json!({});
        fs::write(metadata, package.to_string()).unwrap();
        assert!(
            staged
                .activate()
                .unwrap_err()
                .contains("durable checkpoint")
        );
        assert!(!paths.mcp.exists());

        let staged = stage_mcp(&paths).await.unwrap();
        let attempt = staged.path().parent().unwrap().to_path_buf();
        fs::rename(&attempt, attempt.with_extension("original")).unwrap();
        fs::create_dir(&attempt).unwrap();
        fs::write(attempt.join("marker"), b"replacement").unwrap();
        assert!(
            staged
                .activate()
                .unwrap_err()
                .contains("cleanup refused or failed")
        );
        assert_eq!(fs::read(attempt.join("marker")).unwrap(), b"replacement");
        assert!(!paths.mcp.exists());
    }

    #[tokio::test]
    #[ignore = "requires explicitly provisioned verified private Node.js and network access"]
    async fn live_registry_install_in_isolated_managed_root() {
        let source = PathBuf::from(
            std::env::var_os("FIGCON_TEST_VERIFIED_RUNTIME")
                .expect("Supply a verified isolated runtime fixture"),
        );
        super::super::super::runtime_aliases::validate_runtime_tree(&source).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));
        super::super::super::root::prepare_installation_root(&paths.root).unwrap();
        fn copy(source: &Path, dest: &Path) {
            fs::create_dir(dest).unwrap();
            for entry in fs::read_dir(source).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    copy(&entry.path(), &dest.join(entry.file_name()));
                } else if !entry.file_type().unwrap().is_symlink() {
                    fs::copy(entry.path(), dest.join(entry.file_name())).unwrap();
                }
            }
        }
        copy(&source, &paths.runtime);
        let aliases: Vec<_> = super::super::super::runtime_aliases::APPROVED
            .iter()
            .filter(|alias| {
                fs::symlink_metadata(source.join(alias.name))
                    .is_ok_and(|metadata| metadata.file_type().is_symlink())
            })
            .collect();
        super::super::super::runtime_aliases::reconstruct(&paths.runtime, &aliases).unwrap();
        install_mcp(&paths).await.unwrap();
        let lock = validate_contract(PACKAGE_JSON, PACKAGE_LOCK).unwrap();
        validate_installation(&paths.mcp, &lock).unwrap();
        assert_eq!(fs::read_dir(&paths.staging).unwrap().count(), 1);
    }
}
