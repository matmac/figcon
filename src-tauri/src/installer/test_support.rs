use std::fs;
use std::path::Path;

pub(super) fn node_script() -> String {
    format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo v{}; else echo 11.0.0; fi\n",
        super::manifest::NODE_VERSION
    )
}

pub(super) fn write_runtime(runtime: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(runtime.join("bin")).unwrap();
    fs::write(runtime.join("bin/node"), node_script()).unwrap();
    fs::set_permissions(runtime.join("bin/node"), fs::Permissions::from_mode(0o700)).unwrap();
    fs::create_dir_all(runtime.join("lib/node_modules/npm/bin")).unwrap();
    fs::write(runtime.join(super::npm::NPM_CLI), b"synthetic npm CLI").unwrap();
    fs::write(
        runtime.join("lib/node_modules/npm/package.json"),
        br#"{"name":"npm","version":"11.0.0"}"#,
    )
    .unwrap();
}

pub(super) fn append_npm<W: std::io::Write>(archive: &mut tar::Builder<W>, prefix: &str) {
    for (relative, bytes) in [
        (super::npm::NPM_CLI, b"synthetic npm CLI".as_slice()),
        (
            "lib/node_modules/npm/package.json",
            br#"{"name":"npm","version":"11.0.0"}"#.as_slice(),
        ),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        archive
            .append_data(&mut header, format!("{prefix}/{relative}"), bytes)
            .unwrap();
    }
}
