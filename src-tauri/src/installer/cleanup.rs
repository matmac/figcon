use std::fs;
use std::io;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[cfg(test)]
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

    #[cfg(unix)]
    pub(super) fn handle(&self, path: &Path) -> io::Result<fs::File> {
        self.directories
            .iter()
            .find(|(p, _)| p == path)
            .ok_or_else(|| io::Error::other("Missing directory authority"))?
            .1
            .try_clone()
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

#[derive(Debug, PartialEq, Eq)]
pub(super) enum CleanupDisposition {
    RetainedForReview,
}

pub(super) struct StagingAttempt {
    path: std::path::PathBuf,
    guard: DirectoryGuard,
    #[cfg(target_os = "macos")]
    journal: super::attempt::Journal,
    created: std::time::Instant,
    pub(super) deadline: super::deadline::Deadline,
}

impl StagingAttempt {
    pub(super) fn create(
        paths: &super::paths::InstallerPaths,
        kind: super::attempt::InstallationKind,
    ) -> Result<Self, String> {
        super::deadline::check()?;
        super::root::prepare_installation_root(&paths.root)
            .map_err(|_| "Cannot prepare installation root")?;
        #[cfg(target_os = "macos")]
        let _admission = super::admission::lock(paths)?;
        let report = super::recovery::recover_installations(paths)?;
        super::deadline::check()?;
        if report.orphaned_ledger_files > 0 {
            eprintln!(
                "Recovery retained {} orphaned ledger files for manual review",
                report.orphaned_ledger_files
            );
        }
        if !report.retained.is_empty() {
            eprintln!(
                "Recovery retained {} staging attempts for liveness or manual review",
                report.retained.len()
            );
        }
        if report.retained.len() >= 8 {
            return Err("Too many retained attempts; manual staging review is required".into());
        }
        let path = super::workspace::prepare_installation_workspace(paths)
            .map_err(|_| "Cannot create installation workspace")?;
        Self::capture_typed(paths, path, kind)
            .map_err(|_| "Cannot initialize durable attempt metadata; staging retained".into())
    }

    pub(super) fn capture(
        paths: &super::paths::InstallerPaths,
        path: std::path::PathBuf,
    ) -> io::Result<Self> {
        Self::capture_typed(paths, path, super::attempt::InstallationKind::Probe)
    }

    fn capture_typed(
        paths: &super::paths::InstallerPaths,
        path: std::path::PathBuf,
        kind: super::attempt::InstallationKind,
    ) -> io::Result<Self> {
        if path.parent() != Some(paths.staging.as_path()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Attempt must belong to Figcon staging",
            ));
        }
        let guard = DirectoryGuard::capture(&[&paths.root, &paths.staging, &path])?;
        #[cfg(target_os = "macos")]
        let journal =
            super::attempt::Journal::create(paths, &path, kind).map_err(io::Error::other)?;
        #[cfg(not(target_os = "macos"))]
        {
            let _ = kind;
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Durable attempts require macOS",
            ));
        }
        #[cfg(target_os = "macos")]
        Ok(Self {
            path,
            guard,
            journal,
            created: std::time::Instant::now(),
            deadline: super::deadline::Deadline::current(),
        })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn verify(&self) -> io::Result<()> {
        self.guard.verify()?;
        #[cfg(target_os = "macos")]
        self.journal.verify().map_err(io::Error::other)?;
        Ok(())
    }

    pub(super) fn elapsed(&self) -> std::time::Duration {
        self.created.elapsed()
    }

    pub(super) fn state(&self, state: super::attempt::Lifecycle) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            self.journal.state(state)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = state;
            Err("Durable attempts require macOS".into())
        }
    }

    pub(super) fn checkpoint(&self, state: super::attempt::Lifecycle) -> Result<(), String> {
        self.verify().map_err(|_| "Attempt ownership changed")?;
        #[cfg(target_os = "macos")]
        {
            self.journal.checkpoint(&self.path, state)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = state;
            Err("Durable attempts require macOS".into())
        }
    }

    pub(super) fn verify_checkpoint(&self) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            self.journal.validate_seal(&self.path)
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err("Durable attempts require macOS".into())
        }
    }

    /// Retire the live handle without deleting staging, even if empty or sealed.
    pub(super) fn cleanup(self) -> io::Result<CleanupDisposition> {
        self.retain_at_boundary(|| Ok(()))
    }

    fn retain_at_boundary(
        mut self,
        before_retention: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<CleanupDisposition> {
        // This diagnostic is emitted on success and every refusal/error path.
        // Never include sensitive paths, subprocess output, or configuration.
        eprintln!(
            "Installation staging retained for manual review; automatic staging cleanup is disabled"
        );
        self.verify()?;
        before_retention()?;
        #[cfg(target_os = "macos")]
        self.journal.retain().map_err(io::Error::other)?;
        Ok(CleanupDisposition::RetainedForReview)
    }

    #[cfg(test)]
    pub(super) fn cleanup_with_probe(
        self,
        probe: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<CleanupDisposition> {
        self.retain_at_boundary(probe)
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
        assert!(
            second.exists(),
            "Even an empty completed attempt is retained"
        );
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

#[cfg(all(test, target_os = "macos"))]
mod containment_tests {
    use super::*;
    use crate::installer::{
        anchored, attempt::InstallationKind, attempt_inventory::signature, paths::InstallerPaths,
    };

    fn file_snapshot(root: &Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
        let mut files = std::collections::BTreeMap::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(path) = pending.pop() {
            let metadata = fs::symlink_metadata(&path).unwrap();
            assert!(
                !metadata.file_type().is_symlink(),
                "Fixture must be link-free"
            );
            if metadata.is_dir() {
                pending.extend(
                    fs::read_dir(path)
                        .unwrap()
                        .map(|entry| entry.unwrap().path()),
                );
            } else {
                assert!(metadata.is_file());
                files.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(path).unwrap(),
                );
            }
        }
        files
    }

    #[test]
    fn rename_after_open_retains_every_relocated_file_at_all_ancestor_levels() {
        for level in ["nested", "attempt", "staging", "managed-root"] {
            let temp = tempfile::tempdir().unwrap();
            let paths = InstallerPaths::new(temp.path().join("figcon"));
            let attempt = StagingAttempt::create(&paths, InstallationKind::Mcp).unwrap();
            let attempt_path = attempt.path().to_path_buf();
            let nested = attempt_path.join("nested");
            fs::create_dir_all(nested.join("deeper")).unwrap();
            fs::write(nested.join("first"), b"first-owned-file").unwrap();
            fs::write(nested.join("deeper/second"), b"second-owned-file").unwrap();
            attempt
                .checkpoint(crate::installer::attempt::Lifecycle::Prepared)
                .unwrap();
            let expected = signature(&fs::symlink_metadata(&nested).unwrap());
            let source = match level {
                "nested" => &nested,
                "attempt" => &attempt_path,
                "staging" => &paths.staging,
                _ => &paths.root,
            };
            let mut expected_files = file_snapshot(source);
            let destination = temp.path().join("outside-managed-root");
            let relocated_nested = destination.join(nested.strip_prefix(source).unwrap());
            let paused = std::sync::Barrier::new(2);
            let resume = std::sync::Barrier::new(2);
            std::thread::scope(|scope| {
                let worker = scope.spawn(|| {
                    attempt.cleanup_with_probe(|| {
                        // Open and validate the nested directory, then keep its FD
                        // live over the rename. This is exactly the circumstance
                        // where an FD cannot prove continued staging ancestry.
                        let guard = DirectoryGuard::capture(&[&attempt_path])?;
                        let directory = anchored::open(
                            &guard.handle(&attempt_path)?,
                            std::ffi::OsStr::new("nested"),
                            true,
                        )?;
                        assert_eq!(signature(&directory.metadata()?), expected);
                        assert_eq!(anchored::names(&directory)?.len(), 2);
                        paused.wait();
                        resume.wait();
                        assert!(directory.metadata()?.is_dir());
                        Ok(())
                    })
                });
                paused.wait();
                fs::rename(source, &destination).unwrap();
                fs::write(relocated_nested.join("unexpected"), b"foreign-file").unwrap();
                expected_files.insert(
                    relocated_nested
                        .strip_prefix(&destination)
                        .unwrap()
                        .join("unexpected"),
                    b"foreign-file".to_vec(),
                );
                resume.wait();
                let disposition = worker.join().unwrap();
                if level == "nested" {
                    assert_eq!(disposition.unwrap(), CleanupDisposition::RetainedForReview);
                } else {
                    assert!(
                        disposition.is_err(),
                        "Renamed ownership roots must be reported"
                    );
                }
            });
            assert_eq!(
                file_snapshot(&destination),
                expected_files,
                "Any relocated file changed at {level}"
            );
            for (name, contents) in [
                ("first", b"first-owned-file".as_slice()),
                ("deeper/second", b"second-owned-file".as_slice()),
                ("unexpected", b"foreign-file".as_slice()),
            ] {
                assert_eq!(
                    fs::read(relocated_nested.join(name)).unwrap(),
                    contents,
                    "Relocated file changed at {level}"
                );
            }
        }
    }
}
