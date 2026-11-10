//! Read-only descriptor-relative directory inspection. No deletion primitives:
//! an opened directory can be renamed outside staging without invalidating its FD.
use std::{
    ffi::{CStr, CString},
    fs::File,
    io,
    os::fd::{AsRawFd, FromRawFd},
};
fn name(value: &std::ffi::OsStr) -> io::Result<CString> {
    use std::os::unix::ffi::OsStrExt;
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes == b"." || bytes == b".." || bytes.contains(&b'/') {
        return Err(io::Error::other("Unsafe cleanup component"));
    }
    CString::new(bytes).map_err(|_| io::Error::other("Invalid cleanup component"))
}
pub(super) fn open(
    parent: &File,
    component: &std::ffi::OsStr,
    directory: bool,
) -> io::Result<File> {
    let flags = libc::O_RDONLY
        | libc::O_CLOEXEC
        | libc::O_NOFOLLOW
        | libc::O_NONBLOCK
        | if directory { libc::O_DIRECTORY } else { 0 };
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name(component)?.as_ptr(), flags) };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
pub(super) fn names(directory: &File) -> io::Result<Vec<std::ffi::OsString>> {
    use std::os::unix::ffi::OsStrExt;
    // openat(".") creates an independent directory stream, unlike dup's shared offset.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            c".".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let stream = unsafe { libc::fdopendir(fd) };
    if stream.is_null() {
        unsafe { libc::close(fd) };
        return Err(io::Error::last_os_error());
    }
    let result = (|| {
        let mut result = Vec::new();
        loop {
            super::deadline::check().map_err(io::Error::other)?;
            unsafe { *libc::__error() = 0 };
            let entry = unsafe { libc::readdir(stream) };
            if entry.is_null() {
                if unsafe { *libc::__error() } != 0 {
                    return Err(io::Error::last_os_error());
                }
                break;
            }
            let bytes = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
            if bytes != b"." && bytes != b".." {
                result.push(std::ffi::OsStr::from_bytes(bytes).to_owned());
            }
            if result.len() > 200_000 {
                return Err(io::Error::other("Cleanup entry limit exceeded"));
            }
        }
        result.sort();
        Ok(result)
    })();
    unsafe { libc::closedir(stream) };
    result
}
