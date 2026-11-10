use std::fs;
use std::io;
use std::path::Path;

use super::paths::InstallerPaths;
use super::validate::validate_node_runtime;

pub async fn activate_node_runtime(
    paths: &InstallerPaths,
    staged_runtime: &Path,
) -> io::Result<()> {
    let staging_parent = staged_runtime.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "Invalid staged runtime path")
    })?;

    if staged_runtime.file_name() != Some(std::ffi::OsStr::new("runtime"))
        || staging_parent.parent() != Some(paths.staging.as_path())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Runtime must come from Figcon staging",
        ));
    }

    for directory in [&paths.root, &paths.staging, staging_parent, staged_runtime] {
        let metadata = fs::symlink_metadata(directory)?;

        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Runtime activation requires real directories",
            ));
        }
    }

    match fs::symlink_metadata(&paths.runtime) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "An active runtime already exists",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let directories = super::cleanup::DirectoryGuard::capture(&[
        &paths.root,
        &paths.staging,
        staging_parent,
        staged_runtime,
    ])?;

    validate_node_runtime(staged_runtime)
        .await
        .map_err(io::Error::other)?;

    directories.verify()?;
    super::deadline::check().map_err(io::Error::other)?;
    rename_without_replacement(staged_runtime, &paths.runtime)
}

#[cfg(target_os = "macos")]
pub(super) fn rename_without_replacement(source: &Path, destination: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid source path"))?;

    let destination = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid destination path"))?;

    let result = unsafe {
        libc::renameatx_np(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_EXCL,
        )
    };

    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "macos"))]
pub(super) fn rename_without_replacement(_source: &Path, _destination: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Atomic runtime activation is currently supported only on macOS",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn atomic_rename_preserves_existing_empty_directory() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("marker"), b"source").unwrap();
        fs::create_dir(&destination).unwrap();
        assert!(rename_without_replacement(&source, &destination).is_err());
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
        assert_eq!(fs::read(source.join("marker")).unwrap(), b"source");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn simultaneous_atomic_renames_preserve_winner_and_loser() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        let destination = temp.path().join("runtime");
        for (path, marker) in [
            (&first, b"first".as_slice()),
            (&second, b"second".as_slice()),
        ] {
            fs::create_dir(path).unwrap();
            fs::write(path.join("marker"), marker).unwrap();
        }
        let barrier = std::sync::Barrier::new(2);
        let (one, two) = std::thread::scope(|scope| {
            let one = scope.spawn(|| {
                barrier.wait();
                rename_without_replacement(&first, &destination)
            });
            let two = scope.spawn(|| {
                barrier.wait();
                rename_without_replacement(&second, &destination)
            });
            (one.join().unwrap(), two.join().unwrap())
        });
        assert_ne!(one.is_ok(), two.is_ok());
        let (winner, loser, winner_marker, loser_marker) = if one.is_ok() {
            (&first, &second, b"first".as_slice(), b"second".as_slice())
        } else {
            (&second, &first, b"second".as_slice(), b"first".as_slice())
        };
        assert!(!winner.exists());
        assert_eq!(fs::read(destination.join("marker")).unwrap(), winner_marker);
        assert_eq!(fs::read(loser.join("marker")).unwrap(), loser_marker);
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn activates_staged_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));

        let staged = paths.staging.join("install-test").join("runtime");
        fs::create_dir_all(&staged).unwrap();

        super::super::test_support::write_runtime(&staged);

        activate_node_runtime(&paths, &staged).await.unwrap();

        assert!(paths.runtime.join("bin/node").exists());
        assert!(!staged.exists());
    }

    #[tokio::test]
    async fn refuses_to_replace_existing_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));

        let staged = paths.staging.join("install-test").join("runtime");
        fs::create_dir_all(&staged).unwrap();

        fs::create_dir(&paths.runtime).unwrap();
        fs::write(paths.runtime.join("existing"), b"preserved").unwrap();

        let result = activate_node_runtime(&paths, &staged).await;

        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(
            fs::read(paths.runtime.join("existing")).unwrap(),
            b"preserved"
        );
        assert!(staged.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn refuses_symbolic_link_destination() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));

        let staged = paths.staging.join("install-test").join("runtime");
        fs::create_dir_all(&staged).unwrap();

        let external = temp.path().join("external");
        fs::create_dir(&external).unwrap();
        symlink(&external, &paths.runtime).unwrap();

        let result = activate_node_runtime(&paths, &staged).await;

        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert!(staged.exists());
        assert_eq!(fs::read_dir(&external).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn refuses_dangling_symbolic_link_destination() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));

        let staged = paths.staging.join("install-test").join("runtime");
        fs::create_dir_all(&staged).unwrap();

        let missing = temp.path().join("missing");
        symlink(&missing, &paths.runtime).unwrap();

        let result = activate_node_runtime(&paths, &staged).await;

        assert!(result.is_err());
        assert!(staged.exists());
        assert!(
            fs::symlink_metadata(&paths.runtime)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(!missing.exists());
    }

    #[tokio::test]
    async fn refuses_runtime_outside_staging() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));

        let outside = temp.path().join("runtime");
        fs::create_dir(&outside).unwrap();

        assert!(activate_node_runtime(&paths, &outside).await.is_err());
        assert!(outside.exists());
        assert!(!paths.runtime.exists());
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn concurrent_activations_only_allow_one_winner() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon"));

        let create_runtime = |name: &str| {
            let staged = paths.staging.join(name).join("runtime");
            super::super::test_support::write_runtime(&staged);

            staged
        };

        let first = create_runtime("install-first");
        let second = create_runtime("install-second");

        let (first_result, second_result) = tokio::join!(
            activate_node_runtime(&paths, &first),
            activate_node_runtime(&paths, &second)
        );

        assert_ne!(first_result.is_ok(), second_result.is_ok());

        assert!(paths.runtime.join("bin/node").exists());

        assert!(
            first.exists() || second.exists(),
            "The losing installation must remain available for controlled cleanup"
        );
    }
}
