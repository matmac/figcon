use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub fn create_staging_directory(staging_root: &Path) -> io::Result<PathBuf> {
    let installation_root = staging_root.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Staging directory must have a parent",
        )
    })?;

    let root_metadata = fs::symlink_metadata(installation_root)?;

    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Installation root must be a real directory",
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if root_metadata.uid() != unsafe { libc::geteuid() }
            || root_metadata.permissions().mode() & 0o777 != 0o700
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Installation root must be owned by the current user with 0700 permissions",
            ));
        }
    }

    match fs::symlink_metadata(staging_root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Staging root must not be a symbolic link",
            ));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Staging root must be a directory",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);

        match builder.create(staging_root) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }

        let metadata = fs::symlink_metadata(staging_root)?;

        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Staging root must be a real directory",
            ));
        }

        if metadata.permissions().mode() & 0o777 != 0o700 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Staging root must have 0700 permissions",
            ));
        }

        use std::os::unix::fs::MetadataExt;

        let owner = fs::symlink_metadata(installation_root)?;
        if !owner.is_dir() || owner.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Installation root must be a real directory",
            ));
        }

        if owner.uid() != unsafe { libc::geteuid() } {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Installation root must belong to the current user",
            ));
        }

        if owner.permissions().mode() & 0o777 != 0o700 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Installation root must have 0700 permissions",
            ));
        }

        if metadata.uid() != owner.uid() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Staging root owner does not match installation root owner",
            ));
        }
    }

    #[cfg(not(unix))]
    fs::create_dir(staging_root).or_else(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            Ok(())
        } else {
            Err(error)
        }
    })?;

    for _ in 0..10 {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();

        let sequence = NEXT_ID.fetch_add(1, Ordering::Relaxed);

        let directory = staging_root.join(format!(
            "install-{}-{timestamp}-{sequence}",
            std::process::id()
        ));

        #[cfg(unix)]
        let creation_result = {
            use std::os::unix::fs::DirBuilderExt;

            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            builder.create(&directory)
        };

        #[cfg(not(unix))]
        let creation_result = fs::create_dir(&directory);

        match creation_result {
            Ok(()) => return Ok(directory),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "Could not allocate a unique staging directory",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_distinct_private_directories() {
        let temp = tempfile::tempdir().unwrap();
        let installation_root = temp.path().join("figcon");
        #[cfg(unix)]
        super::super::root::prepare_installation_root(&installation_root).unwrap();
        #[cfg(not(unix))]
        fs::create_dir(&installation_root).unwrap();
        let root = installation_root.join("staging");

        let first = create_staging_directory(&root).unwrap();
        let second = create_staging_directory(&root).unwrap();

        assert_ne!(first, second);
        assert!(first.is_dir());
        assert!(second.is_dir());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&first).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&second).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symbolic_link_staging_roots() {
        use std::os::unix::fs::symlink;

        let base = std::env::temp_dir().join(format!(
            "figcon-staging-symlink-test-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));

        fs::create_dir(&base).unwrap();

        let actual = base.join("actual");
        fs::create_dir(&actual).unwrap();

        let linked = base.join("linked");
        symlink(&actual, &linked).unwrap();

        let result = create_staging_directory(&linked);
        assert!(result.is_err());

        let dangling = base.join("dangling");
        symlink(base.join("missing"), &dangling).unwrap();

        let result = create_staging_directory(&dangling);
        assert!(result.is_err());

        assert_eq!(fs::read_dir(&actual).unwrap().count(), 0);
        assert!(!base.join("missing").exists());

        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symbolic_link_installation_root() {
        use std::os::unix::fs::symlink;

        let base = std::env::temp_dir().join(format!(
            "figcon-install-root-test-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));

        fs::create_dir(&base).unwrap();

        let actual = base.join("actual");
        fs::create_dir(&actual).unwrap();

        let linked = base.join("linked");
        symlink(&actual, &linked).unwrap();

        let result = create_staging_directory(&linked.join("staging"));

        assert!(result.is_err());
        assert!(!actual.join("staging").exists());

        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_insecure_staging_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let base = std::env::temp_dir().join(format!(
            "figcon-staging-permissions-test-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));

        fs::create_dir(&base).unwrap();

        let staging = base.join("staging");
        fs::create_dir(&staging).unwrap();
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o755)).unwrap();

        let result = create_staging_directory(&staging);

        assert!(result.is_err());
        assert_eq!(
            fs::metadata(&staging).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);

        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_insecure_installation_root_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let base = std::env::temp_dir().join(format!(
            "figcon-root-permissions-test-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));

        fs::create_dir(&base).unwrap();

        let root = base.join("figcon");
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();

        let staging = root.join("staging");
        fs::create_dir(&staging).unwrap();
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o700)).unwrap();

        let result = create_staging_directory(&staging);

        assert!(result.is_err());
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);

        fs::remove_dir_all(base).unwrap();
    }
}
