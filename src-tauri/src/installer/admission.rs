//! Cross-process admission lock for cooperating Figcon instances.
use std::{
    fs::File,
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    time::Duration,
};
pub(super) fn lock(paths: &super::paths::InstallerPaths) -> Result<File, String> {
    super::attempt_inventory::private_directory(&paths.metadata)?;
    let guard = super::cleanup::DirectoryGuard::capture(&[&paths.root, &paths.metadata])
        .map_err(|_| "Unsafe admission roots")?;
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(paths.metadata.join("admission.lock"))
        .map_err(|_| "Cannot open admission lock")?;
    let m = file
        .metadata()
        .map_err(|_| "Cannot inspect admission lock")?;
    if !m.is_file()
        || m.nlink() != 1
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o777 != 0o600
    {
        return Err("Unsafe admission lock".into());
    }
    loop {
        super::deadline::check()?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            break;
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::EWOULDBLOCK) {
            return Err("Cannot acquire admission lock".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    guard.verify().map_err(|_| "Admission root changed")?;
    let current = std::fs::symlink_metadata(paths.metadata.join("admission.lock"))
        .map_err(|_| "Admission lock vanished")?;
    if current.dev() != m.dev() || current.ino() != m.ino() {
        return Err("Admission lock replaced".into());
    }
    Ok(file)
}
