use super::*;
use std::fs;

use flate2::{Compression, write::GzEncoder};
use tar::{Builder, EntryType, Header};

fn archive(
    path: &Path,
    architecture: &str,
    links: &[(&str, &str)],
    omit: Option<&str>,
    children: bool,
) {
    let encoder = GzEncoder::new(fs::File::create(path).unwrap(), Compression::default());
    let mut builder = Builder::new(encoder);
    let prefix = format!("node-v{NODE_VERSION}-darwin-{architecture}");
    // Put aliases first to prove no archive entry can traverse a link created
    // during extraction. All regular files are processed before reconstruction.
    for (name, target) in links {
        let mut header = Header::new_gnu();
        header.set_entry_type(EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_link_name(target).unwrap();
        header.set_cksum();
        builder
            .append_data(&mut header, format!("{prefix}/{name}"), &[][..])
            .unwrap();
    }
    let mut files = vec!["bin/node"];
    files.extend(APPROVED.iter().map(|alias| alias.file_target));
    if children {
        files.push("bin/npm/injected");
    }
    for name in files.into_iter().filter(|name| Some(*name) != omit) {
        let bytes = b"synthetic executable";
        let mut header = Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, format!("{prefix}/{name}"), &bytes[..])
            .unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap();
}

#[test]
fn only_exact_release_name_and_target_pairs_are_approved() {
    for arch in ["arm64", "x64"] {
        for alias in &APPROVED {
            let name = format!("node-v{NODE_VERSION}-darwin-{arch}/{}", alias.name);
            assert!(
                from_archive(
                    name.as_bytes(),
                    alias.link_target.as_bytes(),
                    NODE_VERSION,
                    arch
                )
                .is_ok()
            );
            for target in [
                "/etc/passwd",
                "../../outside",
                "../lib/../../outside",
                "../lib/node_modules/npm/bin/./npm-cli.js",
                "../bin/node",
                "",
            ] {
                assert!(
                    from_archive(name.as_bytes(), target.as_bytes(), NODE_VERSION, arch).is_err()
                );
            }
            assert!(
                from_archive(
                    format!("./{name}").as_bytes(),
                    alias.link_target.as_bytes(),
                    NODE_VERSION,
                    arch
                )
                .is_err()
            );
            assert!(
                from_archive(
                    name.as_bytes(),
                    alias.link_target.as_bytes(),
                    "24.22.1",
                    arch
                )
                .is_err()
            );
        }
    }
    assert!(
        from_archive(
            b"node-v24.22.2-darwin-arm64/bin/other",
            APPROVED[0].link_target.as_bytes(),
            NODE_VERSION,
            "arm64"
        )
        .is_err()
    );
    assert!(
        from_archive(
            b"node-v24.22.2-darwin-unsupported/bin/npm",
            APPROVED[0].link_target.as_bytes(),
            NODE_VERSION,
            "unsupported"
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn reconstructs_official_aliases_only_after_regular_file_extraction() {
    for architecture in ["arm64", "x64"] {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("node.tar.gz");
        let links: Vec<_> = APPROVED.iter().map(|a| (a.name, a.link_target)).collect();
        archive(&file, architecture, &links, None, false);
        let runtime = temp.path().join("runtime");
        super::super::extract::extract_node_archive(&file, &runtime, NODE_VERSION, architecture)
            .unwrap();
        for alias in &APPROVED {
            assert_eq!(
                fs::read_link(runtime.join(alias.name)).unwrap().as_os_str(),
                OsStr::new(alias.link_target)
            );
            assert_eq!(
                fs::read(runtime.join(alias.name)).unwrap(),
                b"synthetic executable"
            );
        }
        validate_runtime_tree(&runtime).unwrap();
        // Never grant the runtime's exception to MCP package trees.
        assert!(super::super::mcp_fs::validate_tree(&runtime).is_err());
    }
}

#[test]
fn rejects_unexpected_targets_names_duplicates_and_missing_targets() {
    let official = APPROVED[0].link_target;
    for links in [
        vec![("bin/npm", "/tmp/outside")],
        vec![("bin/npm", "../../outside")],
        vec![("bin/npm", "../lib/node_modules/npm/bin/npx-cli.js")],
        vec![("bin/other", official)],
        vec![("lib/unsafe", official)],
        vec![("bin/npm", official), ("bin/npm", official)],
    ] {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("node.tar.gz");
        let runtime = temp.path().join("runtime");
        archive(&file, "arm64", &links, None, false);
        assert!(
            super::super::extract::extract_node_archive(&file, &runtime, NODE_VERSION, "arm64")
                .is_err()
        );
        assert!(!runtime.exists());
        assert!(!temp.path().join("outside").exists());
    }
    for children in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("node.tar.gz");
        let runtime = temp.path().join("runtime");
        archive(
            &file,
            "arm64",
            &[("bin/npm", official)],
            if children {
                None
            } else {
                Some(APPROVED[0].file_target)
            },
            children,
        );
        assert!(
            super::super::extract::extract_node_archive(&file, &runtime, NODE_VERSION, "arm64")
                .is_err()
        );
        assert!(!runtime.exists());
    }
}

#[cfg(unix)]
#[test]
fn reconstruction_and_runtime_validation_reject_filesystem_escapes() {
    let temp = tempfile::tempdir().unwrap();
    super::super::test_support::write_runtime(temp.path());
    let forged = RuntimeAlias {
        name: "bin/other",
        link_target: "/etc/passwd",
        file_target: APPROVED[0].file_target,
    };
    assert!(reconstruct(temp.path(), &[&forged]).is_err());
    let target = temp.path().join(APPROVED[0].file_target);
    fs::remove_file(&target).unwrap();
    fs::create_dir(&target).unwrap();
    assert!(reconstruct(temp.path(), &[&APPROVED[0]]).is_err());
    fs::remove_dir(&target).unwrap();
    fs::write(&target, b"fixture").unwrap();
    let bin = temp.path().join("lib/node_modules/npm/bin");
    fs::rename(&bin, temp.path().join("original-bin")).unwrap();
    std::os::unix::fs::symlink(temp.path().join("original-bin"), &bin).unwrap();
    assert!(reconstruct(temp.path(), &[&APPROVED[0]]).is_err());
    fs::remove_file(&bin).unwrap();
    fs::rename(temp.path().join("original-bin"), bin).unwrap();
    std::os::unix::fs::symlink("/etc/passwd", temp.path().join("bin/npm")).unwrap();
    assert!(validate_runtime_tree(temp.path()).is_err());
}

#[test]
fn rejects_alias_headers_with_nonzero_payloads() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("node.tar.gz");
    let encoder = GzEncoder::new(fs::File::create(&file).unwrap(), Compression::default());
    let mut builder = Builder::new(encoder);
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Symlink);
    header.set_size(1);
    header.set_mode(0o777);
    header.set_link_name(APPROVED[0].link_target).unwrap();
    header.set_cksum();
    builder
        .append_data(&mut header, "node-v24.22.2-darwin-arm64/bin/npm", &b"x"[..])
        .unwrap();
    builder.into_inner().unwrap().finish().unwrap();
    let runtime = temp.path().join("runtime");
    assert!(
        super::super::extract::extract_node_archive(&file, &runtime, NODE_VERSION, "arm64")
            .unwrap_err()
            .contains("must not contain file data")
    );
    assert!(!runtime.exists());
}
