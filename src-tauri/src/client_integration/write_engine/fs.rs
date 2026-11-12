//! macOS-only descriptor-relative file operations. Never recursively deletes.
use super::{Code, Result};
use serde::{Deserialize, Serialize};
use std::{
    ffi::{CString, OsStr},
    fs::File,
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
    path::{Component, Path, PathBuf},
};
const LIMIT: u64 = 4 * 1024 * 1024;
unsafe extern "C" {
    fn acl_get_fd_np(fd: libc::c_int, kind: libc::c_uint) -> *mut libc::c_void;
    fn acl_get_entry(
        acl: *mut libc::c_void,
        which: libc::c_int,
        entry: *mut *mut libc::c_void,
    ) -> libc::c_int;
    fn acl_free(acl: *mut libc::c_void) -> libc::c_int;
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    dev: u64,
    ino: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    size: u64,
    mtime: i64,
    mtime_ns: i64,
    ctime: i64,
    ctime_ns: i64,
}
impl Identity {
    fn of(m: &std::fs::Metadata) -> Self {
        Self {
            dev: m.dev(),
            ino: m.ino(),
            mode: m.mode(),
            uid: m.uid(),
            gid: m.gid(),
            size: m.len(),
            mtime: m.mtime(),
            mtime_ns: m.mtime_nsec(),
            ctime: m.ctime(),
            ctime_ns: m.ctime_nsec(),
        }
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Stamp {
    pub identity: Identity,
    pub sha256: String,
}
pub(super) struct Snapshot {
    pub stamp: Stamp,
    pub text: String,
    pub mode: u32,
}
fn name(name: &OsStr) -> Result<CString> {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes == b"." || bytes == b".." || bytes.contains(&b'/') {
        return Err(Code::Unsafe);
    }
    CString::new(bytes).map_err(|_| Code::Unsafe)
}
fn metadata(file: &File) -> Result<std::fs::Metadata> {
    file.metadata().map_err(|_| Code::Io)
}
fn safe_dir(file: &File) -> Result<()> {
    let m = metadata(file)?;
    let uid = unsafe { libc::geteuid() };
    if !m.is_dir()
        || (m.uid() != uid && m.uid() != 0)
        || (m.mode() & 0o022 != 0 && !(m.uid() == 0 && m.mode() & 0o1000 != 0))
    {
        return Err(Code::Unsafe);
    }
    Ok(())
}
fn no_extra_metadata(file: &File) -> Result<()> {
    let length = unsafe { libc::flistxattr(file.as_raw_fd(), std::ptr::null_mut(), 0, 0) };
    if !(0..=1024).contains(&length) {
        return Err(Code::Unsupported);
    }
    if length > 0 {
        let mut names = vec![0u8; length as usize];
        if unsafe { libc::flistxattr(file.as_raw_fd(), names.as_mut_ptr().cast(), names.len(), 0) }
            != length
        {
            return Err(Code::Changed);
        }
        // macOS automatically attaches this bookkeeping attribute even to newly
        // created temporary files. It is not an ACL, user setting or capability.
        // Preserve POSIX permission semantics; reject every other xattr rather
        // than silently dropping unsupported security/resource-fork metadata.
        if names != b"com.apple.provenance\0" {
            return Err(Code::Unsupported);
        }
    }
    let acl = unsafe { acl_get_fd_np(file.as_raw_fd(), 0x100) };
    if acl.is_null() {
        return if std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
            Ok(())
        } else {
            Err(Code::Unsupported)
        };
    }
    let mut entry = std::ptr::null_mut();
    let count = unsafe { acl_get_entry(acl, 0, &mut entry) };
    let error = std::io::Error::last_os_error().raw_os_error();
    unsafe { acl_free(acl) };
    // Darwin returns 0 for an entry and -1/ENOENT when the ACL is empty.
    if count == 0 {
        return Err(Code::Unsupported);
    }
    if error != Some(libc::ENOENT) {
        return Err(Code::Unsupported);
    }
    Ok(())
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Anchor {
    dev: u64,
    ino: u64,
    mode: u32,
    uid: u32,
    gid: u32,
}
pub(super) struct Dir {
    file: File,
    path: PathBuf,
    identity: Identity,
}
impl Dir {
    pub fn anchor(&self) -> Anchor {
        Anchor {
            dev: self.identity.dev,
            ino: self.identity.ino,
            mode: self.identity.mode,
            uid: self.identity.uid,
            gid: self.identity.gid,
        }
    }

    pub fn open(path: &Path) -> Result<Self> {
        if !path.is_absolute() {
            return Err(Code::Unsafe);
        }
        let mut file = File::open("/").map_err(|_| Code::Io)?;
        for part in path.components().skip(1) {
            let Component::Normal(component) = part else {
                return Err(Code::Unsafe);
            };
            file = openat(&file, component, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
            safe_dir(&file)?;
        }
        let identity = Identity::of(&metadata(&file)?);
        Ok(Self {
            file,
            path: path.into(),
            identity,
        })
    }
    pub fn check(&self) -> Result<()> {
        let now = Self::open(&self.path)?;
        if now.anchor() != self.anchor() {
            return Err(Code::Changed);
        }
        Ok(())
    }
    pub fn writable(&self) -> Result<()> {
        let m = metadata(&self.file)?;
        if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o200 == 0 {
            return Err(Code::Permission);
        }
        no_extra_metadata(&self.file)
    }
    pub fn private(&self) -> Result<()> {
        let m = metadata(&self.file)?;
        if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o777 != 0o700 {
            return Err(Code::Unsafe);
        }
        no_extra_metadata(&self.file)
    }
    pub fn available_bytes(&self) -> Result<u64> {
        let mut info = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if unsafe { libc::fstatvfs(self.file.as_raw_fd(), info.as_mut_ptr()) } != 0 {
            return Err(Code::Io);
        }
        let info = unsafe { info.assume_init() };
        (info.f_bavail as u64)
            .checked_mul(info.f_frsize as u64)
            .ok_or(Code::Unsupported)
    }
    pub fn existing_child(&self, child: &str) -> Result<Self> {
        let file = openat(
            &self.file,
            OsStr::new(child),
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )?;
        safe_dir(&file)?;
        Ok(Self {
            identity: Identity::of(&metadata(&file)?),
            file,
            path: self.path.join(child),
        })
    }
    pub fn private_entry(&self, child: &str) -> Result<(u64, bool)> {
        let file = openat(&self.file, OsStr::new(child), libc::O_RDONLY, 0)?;
        let m = metadata(&file)?;
        no_extra_metadata(&file)?;
        if m.is_dir() {
            if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o777 != 0o700 {
                return Err(Code::Unsafe);
            }
            return Ok((0, true));
        }
        regular(&file, false)?;
        if m.mode() & 0o777 != 0o600 {
            return Err(Code::Unsafe);
        }
        Ok((m.len(), false))
    }
    pub fn names(&self, limit: usize) -> Result<Vec<String>> {
        use std::ffi::CStr;
        let fd = unsafe {
            libc::openat(
                self.file.as_raw_fd(),
                c".".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(Code::Io);
        }
        let stream = unsafe { libc::fdopendir(fd) };
        if stream.is_null() {
            unsafe { libc::close(fd) };
            return Err(Code::Io);
        }
        let work = || -> Result<Vec<String>> {
            let mut names = Vec::new();
            loop {
                unsafe { *libc::__error() = 0 };
                let entry = unsafe { libc::readdir(stream) };
                if entry.is_null() {
                    if std::io::Error::last_os_error().raw_os_error() != Some(0) {
                        return Err(Code::Io);
                    }
                    break;
                }
                let bytes = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
                if bytes == b"." || bytes == b".." {
                    continue;
                }
                if names.len() >= limit {
                    return Err(Code::Unsupported);
                }
                names.push(
                    std::str::from_utf8(bytes)
                        .map_err(|_| Code::Unsupported)?
                        .into(),
                );
            }
            Ok(names)
        };
        let result = work();
        unsafe { libc::closedir(stream) };
        result
    }
    pub fn sync(&self) -> Result<()> {
        self.file.sync_all().map_err(|_| Code::Io)
    }
    pub fn child(&self, child: &str) -> Result<Self> {
        let n = name(OsStr::new(child))?;
        let result = unsafe { libc::mkdirat(self.file.as_raw_fd(), n.as_ptr(), 0o700) };
        if result != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST) {
            return Err(Code::Io);
        }
        let file = openat(
            &self.file,
            OsStr::new(child),
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )?;
        safe_dir(&file)?;
        let child = Self {
            identity: Identity::of(&metadata(&file)?),
            file,
            path: self.path.join(child),
        };
        child.private()?;
        self.sync()?;
        Ok(child)
    }
    pub fn lock(&self, child: &str) -> Result<File> {
        let file = match openat(
            &self.file,
            OsStr::new(child),
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
            0o600,
        ) {
            Ok(file) => file,
            Err(Code::Exists) => openat(&self.file, OsStr::new(child), libc::O_RDWR, 0)?,
            Err(error) => return Err(error),
        };
        regular(&file, true)?;
        if metadata(&file)?.mode() & 0o777 != 0o600 {
            return Err(Code::Unsafe);
        }
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(Code::Busy);
        }
        let current = openat(&self.file, OsStr::new(child), libc::O_RDONLY, 0)?;
        let held = metadata(&file)?;
        let named = metadata(&current)?;
        if held.dev() != named.dev() || held.ino() != named.ino() || held.nlink() != 1 {
            return Err(Code::Changed);
        }
        self.sync()?;
        Ok(file)
    }
    pub fn read(&self, child: &str) -> Result<Option<Snapshot>> {
        let file = match openat(&self.file, OsStr::new(child), libc::O_RDONLY, 0) {
            Ok(f) => f,
            Err(Code::Missing) => return Ok(None),
            Err(e) => return Err(e),
        };
        regular(&file, false)?;
        let before = metadata(&file)?;
        no_extra_metadata(&file)?;
        if before.len() > LIMIT {
            return Err(Code::Unsupported);
        }
        let mut bytes = Vec::new();
        (&file)
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Code::Io)?;
        let after = metadata(&file)?;
        if Identity::of(&before) != Identity::of(&after) || bytes.len() as u64 > LIMIT {
            return Err(Code::Changed);
        }
        Ok(Some(Snapshot {
            stamp: Stamp {
                identity: Identity::of(&after),
                sha256: super::hash(&bytes),
            },
            text: String::from_utf8(bytes).map_err(|_| Code::Malformed)?,
            mode: after.mode() & 0o777,
        }))
    }
    pub fn fingerprint(&self, child: &str) -> Result<Stamp> {
        use sha2::{Digest, Sha256};
        let mut file = openat(&self.file, OsStr::new(child), libc::O_RDONLY, 0)?;
        regular(&file, false)?;
        no_extra_metadata(&file)?;
        let before = metadata(&file)?;
        let limit = 256 * 1024 * 1024;
        if before.len() == 0 || before.len() > limit {
            return Err(Code::Unsupported);
        }
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];
        let mut count = 0u64;
        loop {
            let n = file.read(&mut buffer).map_err(|_| Code::Io)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            if count > limit {
                return Err(Code::Unsupported);
            }
            hasher.update(&buffer[..n]);
        }
        if Identity::of(&before) != Identity::of(&metadata(&file)?) {
            return Err(Code::Changed);
        }
        Ok(Stamp {
            identity: Identity::of(&before),
            sha256: format!("{:x}", hasher.finalize()),
        })
    }
    pub fn create(&self, child: &str, bytes: &[u8], mode: u32) -> Result<()> {
        self.check()?;
        let mut file = openat(
            &self.file,
            OsStr::new(child),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?;
        file.write_all(bytes).map_err(|_| Code::Io)?;
        if unsafe { libc::fchmod(file.as_raw_fd(), mode as libc::mode_t) } != 0 {
            return Err(Code::Permission);
        }
        file.sync_all().map_err(|_| Code::Io)?;
        self.sync()
    }
    pub fn publication_handle(&self, child: &str, expected: &Stamp) -> Result<File> {
        let file = openat(&self.file, OsStr::new(child), libc::O_RDWR, 0)?;
        regular(&file, true)?;
        if Identity::of(&metadata(&file)?) != expected.identity {
            return Err(Code::Changed);
        }
        Ok(file)
    }
    pub fn finish_mode(file: &File, mode: u32) -> Result<()> {
        if unsafe { libc::fchmod(file.as_raw_fd(), mode as libc::mode_t) } != 0 {
            return Err(Code::Permission);
        }
        file.sync_all().map_err(|_| Code::Io)
    }
    pub fn publish(&self, temp: &str, target: &str, missing: bool) -> Result<()> {
        let a = name(OsStr::new(temp))?;
        let b = name(OsStr::new(target))?;
        let result = unsafe {
            libc::renameatx_np(
                self.file.as_raw_fd(),
                a.as_ptr(),
                self.file.as_raw_fd(),
                b.as_ptr(),
                if missing { libc::RENAME_EXCL } else { 0 },
            )
        };
        if result != 0 {
            return Err(
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::EEXIST) {
                    Code::Changed
                } else {
                    Code::Io
                },
            );
        }
        Ok(())
    }
}
fn openat(parent: &File, component: &OsStr, flags: i32, mode: u32) -> Result<File> {
    let n = name(component)?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            n.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            mode as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ENOENT) => Code::Missing,
            Some(libc::EEXIST) => Code::Exists,
            Some(libc::EACCES | libc::EPERM) => Code::Permission,
            Some(libc::ELOOP | libc::ENOTDIR) => Code::Unsafe,
            _ => Code::Io,
        });
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn regular(file: &File, writable: bool) -> Result<()> {
    let m = metadata(file)?;
    if !m.is_file()
        || m.nlink() != 1
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o022 != 0
        || std::os::macos::fs::MetadataExt::st_flags(&m) != 0
        || m.mode() & 0o7000 != 0
    {
        return Err(Code::Unsafe);
    }
    if m.mode() & 0o400 == 0 || (writable && m.mode() & 0o200 == 0) {
        return Err(Code::Permission);
    }
    Ok(())
}
pub(super) fn require_writable(snapshot: &Snapshot) -> Result<()> {
    if snapshot.mode & 0o200 == 0 {
        return Err(Code::Permission);
    }
    Ok(())
}

pub(super) fn same_permissions(before: &Snapshot, after: &Snapshot) -> Result<()> {
    if before.mode != after.mode
        || before.stamp.identity.uid != after.stamp.identity.uid
        || before.stamp.identity.gid != after.stamp.identity.gid
    {
        return Err(Code::Unsupported);
    }
    Ok(())
}

pub(super) fn published_matches(before: &Snapshot, after: &Snapshot) -> bool {
    before.stamp.identity.dev == after.stamp.identity.dev
        && before.stamp.identity.ino == after.stamp.identity.ino
        && before.stamp.sha256 == after.stamp.sha256
        && same_owners(before, after).is_ok()
}

pub(super) fn same_owners(before: &Snapshot, after: &Snapshot) -> Result<()> {
    if before.stamp.identity.uid != after.stamp.identity.uid
        || before.stamp.identity.gid != after.stamp.identity.gid
    {
        return Err(Code::Unsupported);
    }
    Ok(())
}
