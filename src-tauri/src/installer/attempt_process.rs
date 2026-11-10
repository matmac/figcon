use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ProcessIdentity {
    pub pid: u32,
    pub start_seconds: u64,
    pub start_microseconds: u64,
    pub boot_session: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Liveness {
    Live,
    Dead,
    Unknown,
}

#[cfg(target_os = "macos")]
fn boot_session() -> Result<String, String> {
    let mut buffer = [0u8; 64];
    let mut size = buffer.len();
    let result = unsafe {
        libc::sysctlbyname(
            c"kern.bootsessionuuid".as_ptr(),
            buffer.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 || size < 2 || size > buffer.len() {
        return Err("Cannot establish macOS boot identity".into());
    }
    let value =
        std::str::from_utf8(&buffer[..size - 1]).map_err(|_| "Invalid macOS boot identity")?;
    if value.len() != 36 || !value.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return Err("Invalid macOS boot identity".into());
    }
    Ok(value.to_owned())
}

#[cfg(target_os = "macos")]
fn info(pid: u32) -> Result<libc::proc_bsdinfo, Liveness> {
    if pid == 0 || pid > i32::MAX as u32 {
        return Err(Liveness::Unknown);
    }
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = std::mem::size_of::<libc::proc_bsdinfo>();
    let read = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size as i32,
        )
    };
    if read == size as i32 {
        return Ok(unsafe { info.assume_init() });
    }
    // Signal zero never terminates a process. Permission/inspection failures
    // are unknown, never evidence that an installation is dead.
    if unsafe { libc::kill(pid as i32, 0) } == -1
        && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
    {
        Err(Liveness::Dead)
    } else {
        Err(Liveness::Unknown)
    }
}

impl ProcessIdentity {
    #[cfg(target_os = "macos")]
    pub fn current() -> Result<Self, String> {
        let pid = std::process::id();
        let process = info(pid).map_err(|_| "Cannot establish process birth identity")?;
        Ok(Self {
            pid,
            start_seconds: process.pbi_start_tvsec,
            start_microseconds: process.pbi_start_tvusec,
            boot_session: boot_session()?,
        })
    }

    #[cfg(target_os = "macos")]
    pub fn liveness(&self) -> Liveness {
        let Ok(boot) = boot_session() else {
            return Liveness::Unknown;
        };
        // No cross-boot ownership inference: a forged or stale boot value is
        // not sufficient evidence for deletion. Preserve it for manual review.
        if boot != self.boot_session {
            return Liveness::Unknown;
        }
        match info(self.pid) {
            Ok(process)
                if process.pbi_start_tvsec == self.start_seconds
                    && process.pbi_start_tvusec == self.start_microseconds =>
            {
                Liveness::Live
            }
            // A reused PID does not grant authority over its new process.
            Ok(_) => Liveness::Unknown,
            Err(state) => state,
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) fn descendants_quiet(group: u32) -> Result<bool, String> {
    // PROC_PGRP_ONLY = 2 in the macOS SDK's sys/proc_info.h. This is a
    // macOS-only read; caller must hold the exited leader unreaped.
    let mut pids = [0i32; 4096];
    unsafe {
        *libc::__error() = 0;
    }
    let bytes = unsafe {
        libc::proc_listpids(
            2,
            group,
            pids.as_mut_ptr().cast(),
            std::mem::size_of_val(&pids) as i32,
        )
    };
    let error = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
    if bytes < 0 || bytes as usize >= std::mem::size_of_val(&pids) || (bytes == 0 && error != 0) {
        return Err("Cannot establish installer process-group quiescence".into());
    }
    for pid in pids
        .iter()
        .take(bytes as usize / std::mem::size_of::<i32>())
        .filter(|pid| **pid > 0 && **pid as u32 != group)
    {
        match info(*pid as u32) {
            Ok(process) if process.pbi_status == libc::SZOMB => {}
            Ok(_) => return Ok(false),
            Err(Liveness::Dead) => {}
            Err(_) => return Err("Cannot inspect installer descendant liveness".into()),
        }
    }
    Ok(true)
}
