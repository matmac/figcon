use std::fs;
use std::io;
use std::path::Path;

pub fn prepare_installation_root(root: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};

        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);

        match builder.create(root) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }

        let metadata = fs::symlink_metadata(root)?;

        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Installation root must be a real directory",
            ));
        }

        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Installation root must belong to the current user",
            ));
        }

        if metadata.permissions().mode() & 0o777 != 0o700 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Installation root must have 0700 permissions",
            ));
        }

        return Ok(());
    }

    #[cfg(not(unix))]
    {
        let _ = root;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Secure installation root creation is not implemented on this platform",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn creates_private_installation_root() {
        use std::os::unix::fs::PermissionsExt;

        let base =
            std::env::temp_dir().join(format!("figcon-root-create-test-{}", std::process::id()));

        fs::create_dir(&base).unwrap();

        let root = base.join("figcon");

        prepare_installation_root(&root).unwrap();
        prepare_installation_root(&root).unwrap();

        assert!(root.is_dir());
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );

        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symbolic_link_installation_roots() {
        use std::os::unix::fs::symlink;

        let base =
            std::env::temp_dir().join(format!("figcon-root-symlink-test-{}", std::process::id()));

        fs::create_dir(&base).unwrap();

        let actual = base.join("actual");
        fs::create_dir(&actual).unwrap();

        let linked = base.join("linked");
        symlink(&actual, &linked).unwrap();

        assert!(prepare_installation_root(&linked).is_err());

        let dangling = base.join("dangling");
        symlink(base.join("missing"), &dangling).unwrap();

        assert!(prepare_installation_root(&dangling).is_err());

        assert_eq!(fs::read_dir(&actual).unwrap().count(), 0);
        assert!(!base.join("missing").exists());

        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_insecure_existing_installation_root() {
        use std::os::unix::fs::PermissionsExt;

        let base =
            std::env::temp_dir().join(format!("figcon-root-insecure-test-{}", std::process::id()));

        fs::create_dir(&base).unwrap();

        let root = base.join("figcon");
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();

        let result = prepare_installation_root(&root);

        assert!(result.is_err());
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);

        fs::remove_dir_all(base).unwrap();
    }
}
