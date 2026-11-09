use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

struct ManagedChild {
    child: Child,
    group: Option<u32>,
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

impl ManagedChild {
    fn stop_group(&mut self) -> bool {
        #[cfg(unix)]
        if let Some(pid) = self.group.take() {
            // Each installer command starts a fresh process group. Kill its
            // descendants too, including when its waiting future is dropped.
            if unsafe { libc::kill(-(pid as i32), libc::SIGKILL) } != 0
                && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
            {
                return false;
            }
        }
        true
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        if !self.stop_group() {
            eprintln!("Cannot terminate installer process group");
        }
        let _ = self.child.start_kill();
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
        group: child.id(),
        child,
    };
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
        let status = child
            .child
            .wait()
            .await
            .map_err(|_| "Cannot wait for installer process")?;
        if !status.success() {
            return Err("Installer process failed");
        }
        Ok(output)
    })
    .await;

    if !child.stop_group() {
        let _ = child.child.kill().await;
        return Err("Cannot terminate installer process group".into());
    }
    match result {
        Ok(Ok(output)) => Ok(output),
        failure => {
            let _ = child.child.kill().await;
            match failure {
                Err(_) => Err("Installer process timed out".into()),
                Ok(Err(error)) => Err(error.into()),
                Ok(Ok(_)) => unreachable!(),
            }
        }
    }
}
