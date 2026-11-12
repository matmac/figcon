//! Bounded passive reads; no create, repair, serialization or subprocess calls.
use super::models::ConfigurationStatus;
use std::path::Path;
const LIMIT: u64 = 4 * 1024 * 1024;
#[cfg(target_os = "macos")]
pub(super) fn read(path: &Path) -> Result<String, ConfigurationStatus> {
    use std::{
        ffi::CString,
        fs::File,
        io::Read,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{ffi::OsStrExt, fs::MetadataExt},
        },
        path::Component,
    };
    if !path.is_absolute() {
        return Err(ConfigurationStatus::Unsafe);
    }
    let parts: Vec<_> = path.components().collect();
    let mut file = File::open("/").map_err(|_| ConfigurationStatus::Inaccessible)?;
    for (index, part) in parts.iter().enumerate().skip(1) {
        let Component::Normal(name) = part else {
            return Err(ConfigurationStatus::Unsafe);
        };
        let last = index == parts.len() - 1;
        let name = CString::new(name.as_bytes()).map_err(|_| ConfigurationStatus::Unsafe)?;
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | if last { 0 } else { libc::O_DIRECTORY };
        let fd = unsafe { libc::openat(file.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            let error = std::io::Error::last_os_error();
            return Err(match error.raw_os_error() {
                Some(libc::ENOENT) => ConfigurationStatus::Missing,
                Some(libc::ELOOP | libc::ENOTDIR) => ConfigurationStatus::Unsafe,
                _ => ConfigurationStatus::Inaccessible,
            });
        }
        file = unsafe { File::from_raw_fd(fd) };
        let m = file
            .metadata()
            .map_err(|_| ConfigurationStatus::Inaccessible)?;
        let uid = unsafe { libc::geteuid() };
        if last {
            if !m.is_file() || m.nlink() != 1 || m.uid() != uid || m.mode() & 0o022 != 0 {
                return Err(ConfigurationStatus::Unsafe);
            }
            if m.mode() & 0o400 == 0 {
                return Err(ConfigurationStatus::Inaccessible);
            }
        } else if !m.is_dir()
            || (m.uid() != 0 && m.uid() != uid)
            || (m.mode() & 0o022 != 0 && !(m.uid() == 0 && m.mode() & 0o1000 != 0))
        {
            return Err(ConfigurationStatus::Unsafe);
        }
    }
    let before = file
        .metadata()
        .map_err(|_| ConfigurationStatus::Inaccessible)?;
    if !before.is_file() || before.len() > LIMIT {
        return Err(ConfigurationStatus::Unsafe);
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ConfigurationStatus::Inaccessible)?;
    let after = file
        .metadata()
        .map_err(|_| ConfigurationStatus::Inaccessible)?;
    if bytes.len() as u64 > LIMIT
        || before.len() != after.len()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
    {
        return Err(ConfigurationStatus::Unsafe);
    }
    String::from_utf8(bytes).map_err(|_| ConfigurationStatus::Malformed)
}
#[cfg(not(target_os = "macos"))]
pub(super) fn read(_: &Path) -> Result<String, ConfigurationStatus> {
    let _ = LIMIT;
    Err(ConfigurationStatus::UnsupportedPlatform)
}

// Acceptance discovery must never canonicalize an executable link into host state.
#[cfg(target_os = "macos")]
pub(super) fn synthetic_executable(path: &Path) -> bool {
    use std::{
        ffi::CString,
        fs::File,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{ffi::OsStrExt, fs::MetadataExt},
        },
        path::Component,
    };
    let check = || -> Option<()> {
        if !path.is_absolute() {
            return None;
        }
        let parts: Vec<_> = path.components().collect();
        let mut file = File::open("/").ok()?;
        for (index, part) in parts.iter().enumerate().skip(1) {
            let Component::Normal(name) = part else {
                return None;
            };
            let last = index == parts.len() - 1;
            let name = CString::new(name.as_bytes()).ok()?;
            let flags = libc::O_RDONLY
                | libc::O_NOFOLLOW
                | libc::O_CLOEXEC
                | libc::O_NONBLOCK
                | if last { 0 } else { libc::O_DIRECTORY };
            let fd = unsafe { libc::openat(file.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 {
                return None;
            }
            file = unsafe { File::from_raw_fd(fd) };
        }
        let m = file.metadata().ok()?;
        if !m.is_file()
            || m.nlink() != 1
            || m.uid() != unsafe { libc::geteuid() }
            || m.mode() & 0o111 == 0
            || m.mode() & 0o022 != 0
        {
            return None;
        }
        Some(())
    };
    check().is_some()
}
#[cfg(not(target_os = "macos"))]
pub(super) fn synthetic_executable(_path: &Path) -> bool {
    false
}
