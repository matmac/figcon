use std::fs;
use std::io;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

pub fn remove_if_same_file(path: &Path, original: &fs::Metadata) -> io::Result<bool> {
    let current = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(false);
        }
        Err(error) => return Err(error),
    };

    #[cfg(unix)]
    {
        if !current.is_file() || current.dev() != original.dev() || current.ino() != original.ino()
        {
            return Ok(false);
        }

        fs::remove_file(path)?;
        Ok(true)
    }

    #[cfg(not(unix))]
    {
        let _ = current;
        let _ = original;

        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Safe cleanup is not implemented on this platform",
        ))
    }
}

/// Keeps directory handles open so inode reuse cannot masquerade as ownership.
/// Path checks detect replacement before an operation, but are not a substitute
/// for descriptor-relative operations against a malicious same-user process.
pub(super) struct DirectoryGuard {
    #[cfg(unix)]
    directories: Vec<(std::path::PathBuf, fs::File)>,
}

impl DirectoryGuard {
    pub(super) fn capture(paths: &[&Path]) -> io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut directories = Vec::new();
            for path in paths {
                let handle = fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
                    .open(path)?;
                directories.push((path.to_path_buf(), handle));
            }
            let guard = Self { directories };
            guard.verify()?;
            Ok(guard)
        }
        #[cfg(not(unix))]
        {
            let _ = paths;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Directory ownership checks require Unix",
            ))
        }
    }

    pub(super) fn verify(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            for (path, handle) in &self.directories {
                let current = fs::symlink_metadata(path)?;
                let original = handle.metadata()?;
                if !current.is_dir()
                    || current.file_type().is_symlink()
                    || current.dev() != original.dev()
                    || current.ino() != original.ino()
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Installation directory identity changed",
                    ));
                }
            }
            Ok(())
        }
        #[cfg(not(unix))]
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Directory ownership checks require Unix",
        ))
    }
}

pub(super) struct StagingAttempt {
    path: std::path::PathBuf,
    guard: DirectoryGuard,
}

impl StagingAttempt {
    pub(super) fn capture(
        paths: &super::paths::InstallerPaths,
        path: std::path::PathBuf,
    ) -> io::Result<Self> {
        if path.parent() != Some(paths.staging.as_path()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Attempt must belong to Figcon staging",
            ));
        }
        let guard = DirectoryGuard::capture(&[&paths.root, &paths.staging, &path])?;
        Ok(Self { path, guard })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn verify(&self) -> io::Result<()> {
        self.guard.verify()
    }

    pub(super) fn cleanup(self) -> io::Result<()> {
        self.verify()?;
        fs::remove_dir_all(&self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn staging_cleanup_preserves_replacements_and_other_attempts() {
        let temp = tempfile::tempdir().unwrap();
        let paths = super::super::paths::InstallerPaths::new(temp.path().join("figcon"));
        let first = super::super::workspace::prepare_installation_workspace(&paths).unwrap();
        let second = super::super::workspace::prepare_installation_workspace(&paths).unwrap();
        let attempt = StagingAttempt::capture(&paths, first.clone()).unwrap();
        let moved = paths.staging.join("moved-original");
        fs::rename(&first, &moved).unwrap();
        fs::create_dir(&first).unwrap();
        fs::write(first.join("marker"), b"replacement").unwrap();
        assert!(attempt.cleanup().is_err());
        assert_eq!(fs::read(first.join("marker")).unwrap(), b"replacement");
        assert!(moved.is_dir());
        assert!(second.is_dir());
        StagingAttempt::capture(&paths, second.clone())
            .unwrap()
            .cleanup()
            .unwrap();
        assert!(!second.exists());
        assert!(first.exists());
    }

    #[cfg(unix)]
    #[test]
    fn staging_cleanup_rejects_replaced_staging_ancestor() {
        let temp = tempfile::tempdir().unwrap();
        let paths = super::super::paths::InstallerPaths::new(temp.path().join("figcon"));
        let first = super::super::workspace::prepare_installation_workspace(&paths).unwrap();
        let attempt = StagingAttempt::capture(&paths, first.clone()).unwrap();
        fs::rename(&paths.staging, paths.root.join("old-staging")).unwrap();
        fs::create_dir(&paths.staging).unwrap();
        fs::create_dir(&first).unwrap();
        assert!(attempt.cleanup().is_err());
        assert!(first.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn removes_original_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.tar.gz");

        fs::write(&path, b"original").unwrap();
        let original = fs::metadata(&path).unwrap();

        assert!(remove_if_same_file(&path, &original).unwrap());
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn preserves_replacement_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.tar.gz");
        let old = dir.path().join("old.tar.gz");

        fs::write(&path, b"original").unwrap();
        let original = fs::metadata(&path).unwrap();

        fs::rename(&path, &old).unwrap();
        fs::write(&path, b"replacement").unwrap();

        assert!(!remove_if_same_file(&path, &original).unwrap());
        assert_eq!(fs::read(&path).unwrap(), b"replacement");
    }
}
