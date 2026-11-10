use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

struct ManagedChild {
    child: Child,
    group: std::sync::Arc<Group>,
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn timeout_kills_descendants_without_returning_their_output() {
        let temp = tempfile::tempdir().unwrap();
        let mut command = Command::new("/bin/sh");
        command.current_dir(temp.path()).env_clear().args([
            "-c",
            "echo SYNTHETIC_SECRET >&2; ( /bin/sleep 0.5; echo leaked > child-survived ) & wait",
        ]);
        assert_eq!(
            run(&mut command, Duration::from_millis(100), false)
                .await
                .unwrap_err(),
            "Installer process timed out"
        );
        tokio::time::sleep(Duration::from_millis(750)).await;
        assert!(!temp.path().join("child-survived").exists());
    }

    #[tokio::test]
    async fn version_output_is_bounded() {
        let mut command = Command::new("/bin/sh");
        command.env_clear().args([
            "-c",
            "while :; do echo 012345678901234567890123456789; done",
        ]);
        assert_eq!(
            run(&mut command, Duration::from_secs(2), true)
                .await
                .unwrap_err(),
            "Version probe output exceeded limit"
        );
    }
}

pub(super) struct Group(std::sync::Mutex<Option<u32>>);
impl Group {
    pub(super) fn stop(&self) -> bool {
        let Ok(mut group) = self.0.lock() else {
            return false;
        };
        if let Some(pid) = group.take() {
            #[cfg(unix)]
            if unsafe { libc::kill(-(pid as i32), libc::SIGKILL) } != 0
                && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
            {
                return false;
            }
            #[cfg(not(unix))]
            let _ = pid;
        }
        true
    }
    fn release(&self) {
        if let Ok(mut group) = self.0.lock() {
            *group = None;
        }
    }
}
impl ManagedChild {
    fn stop_group(&mut self) -> bool {
        self.group.stop()
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        if !self.stop_group() {
            eprintln!("Cannot terminate installer process group");
        }
        if self.child.id().is_some() && self.child.start_kill().is_err() {
            eprintln!("Cannot request installer child termination; staging requires manual review");
        }
    }
}

/// Never returns subprocess output in errors. Only version probes capture a
/// small stdout buffer; npm installation/inspection output is discarded.
pub(super) async fn run(
    command: &mut Command,
    timeout: Duration,
    capture_version: bool,
) -> Result<Vec<u8>, String> {
    command
        .stdin(Stdio::null())
        .stdout(if capture_version {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);

    let child = command
        .spawn()
        .map_err(|_| "Cannot start installer process")?;
    let mut child = ManagedChild {
        group: std::sync::Arc::new(Group(std::sync::Mutex::new(child.id()))),
        child,
    };
    #[cfg(target_os = "macos")]
    super::deadline::register_group(&child.group);
    // Keep the pipe open until termination/reaping. Closing it at the read
    // limit can race SIGPIPE exit against process-group termination.
    let mut stdout = child.child.stdout.take();
    let result = tokio::time::timeout(timeout, async {
        let mut output = Vec::new();
        if capture_version {
            stdout
                .as_mut()
                .ok_or("Cannot read version probe")?
                .take(257)
                .read_to_end(&mut output)
                .await
                .map_err(|_| "Cannot read version probe")?;
            if output.len() > 256 {
                return Err("Version probe output exceeded limit");
            }
        }
        #[cfg(target_os = "macos")]
        {
            // Observe exit without reaping. The owned zombie reserves the PID
            // and group identity until group termination has been requested.
            let pid = child.child.id().ok_or("Missing installer group")?;
            loop {
                super::deadline::check()
                    .map_err(|_| "Installation cancelled or deadline exceeded")?;
                let mut information = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
                let result = unsafe {
                    libc::waitid(
                        libc::P_PID,
                        pid,
                        information.as_mut_ptr(),
                        libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                    )
                };
                if result != 0 {
                    return Err("Cannot observe installer exit without reaping");
                }
                if unsafe { information.assume_init().si_pid() } != 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            // Even a successful leader can leave children behind. Kill the
            // group while the unreaped leader still prevents PID recycling.
            if super::attempt_process::descendants_quiet(pid)
                .map_err(|_| "Cannot inspect owned installer descendants")?
            {
                child.group.release();
            } else {
                if !child.stop_group() {
                    return Err("Cannot terminate installer descendants");
                }
                tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        if matches!(super::attempt_process::descendants_quiet(pid), Ok(true)) {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                })
                .await
                .map_err(|_| "Cannot establish installer descendant termination")?;
            }
        }
        let status = tokio::time::timeout(Duration::from_secs(5), child.child.wait())
            .await
            .map_err(|_| "Cannot reap installer process within cleanup deadline")?
            .map_err(|_| "Cannot wait for installer process")?;
        child.group.release();
        if !status.success() {
            return Err("Installer process failed");
        }
        Ok(output)
    })
    .await;

    if !child.stop_group() {
        if child.child.id().is_some()
            && tokio::time::timeout(Duration::from_secs(5), child.child.kill())
                .await
                .map_or(true, |r| r.is_err())
        {
            return Err("Cannot terminate installer process group or reap installer child".into());
        }
        return Err("Cannot terminate installer process group".into());
    }
    match result {
        Ok(Ok(output)) => Ok(output),
        failure => {
            if child.child.id().is_some()
                && tokio::time::timeout(Duration::from_secs(5), child.child.kill())
                    .await
                    .map_or(true, |r| r.is_err())
            {
                return Err("Installer process failed or timed out; child cleanup failed".into());
            }
            match failure {
                Err(_) => Err("Installer process timed out".into()),
                Ok(Err(error)) => Err(error.into()),
                Ok(Ok(_)) => unreachable!(),
            }
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod supervision_tests {
    use super::*;
    #[tokio::test]
    async fn captured_successful_probe_is_reaped_successfully() {
        let mut command = Command::new("/bin/sh");
        command.env_clear().args(["-c", "echo version"]);
        assert_eq!(
            run(&mut command, Duration::from_secs(5), true)
                .await
                .unwrap(),
            b"version\n"
        );
    }
    #[tokio::test]
    async fn exited_leader_does_not_leave_writing_descendants() {
        let temp = tempfile::tempdir().unwrap();
        let mut command = Command::new("/bin/sh");
        command
            .current_dir(temp.path())
            .env_clear()
            .args(["-c", "( /bin/sleep 0.4; echo escaped > survived ) & exit 0"]);
        run(&mut command, Duration::from_secs(5), false)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(!temp.path().join("survived").exists());
    }
}
