use std::io;
use std::path::PathBuf;

use super::paths::InstallerPaths;
use super::root::prepare_installation_root;
use super::staging::create_staging_directory;

pub fn prepare_installation_workspace(paths: &InstallerPaths) -> io::Result<PathBuf> {
    prepare_installation_root(&paths.root)?;
    create_staging_directory(&paths.staging)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn prepares_private_installation_workspace() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

        let base = std::env::temp_dir().join(format!(
            "figcon-workspace-test-{}-{}",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed)
        ));

        fs::create_dir(&base).unwrap();

        let paths = InstallerPaths::new(base.join("figcon"));

        let first = prepare_installation_workspace(&paths).unwrap();
        let second = prepare_installation_workspace(&paths).unwrap();

        assert_ne!(first, second);
        assert!(first.is_dir());
        assert!(second.is_dir());

        for directory in [&paths.root, &paths.staging, &first, &second] {
            assert_eq!(
                fs::metadata(directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }

        assert!(!paths.runtime.exists());
        assert!(!paths.mcp.exists());

        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_workspace_with_insecure_root() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let base =
            std::env::temp_dir().join(format!("figcon-workspace-rejection-{}", std::process::id()));

        fs::create_dir(&base).unwrap();

        let paths = InstallerPaths::new(base.join("figcon"));

        fs::create_dir(&paths.root).unwrap();
        fs::set_permissions(&paths.root, fs::Permissions::from_mode(0o755)).unwrap();

        let result = prepare_installation_workspace(&paths);

        assert!(result.is_err());
        assert!(!paths.staging.exists());
        assert!(!paths.runtime.exists());
        assert!(!paths.mcp.exists());

        fs::remove_dir_all(base).unwrap();
    }
}
