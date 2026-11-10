use std::path::Path;
use std::time::Duration;

const MIB: u64 = 1024 * 1024;

#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub total_bytes: u64,
    pub package_bytes: u64,
    pub cache_bytes: u64,
    pub temporary_bytes: u64,
    pub file_bytes: u64,
    pub entries: usize,
    pub duration: Duration,
    pub interval: Duration,
}

pub(super) const MCP: Limits = Limits {
    total_bytes: 1280 * MIB,
    package_bytes: 512 * MIB,
    cache_bytes: 512 * MIB,
    temporary_bytes: 128 * MIB,
    file_bytes: 64 * MIB,
    entries: 100_000,
    duration: Duration::from_secs(600),
    interval: Duration::from_millis(250),
};

fn disappeared(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound
}

impl Limits {
    pub fn check(&self, attempt: &super::cleanup::StagingAttempt) -> Result<(), String> {
        attempt
            .verify()
            .map_err(|_| "Installation quota ownership check failed")?;
        self.scan(attempt.path(), || attempt.elapsed() >= self.duration)?;
        attempt
            .verify()
            .map_err(|_| "Installation quota ownership changed")?;
        Ok(())
    }

    fn scan(&self, root: &Path, expired: impl Fn() -> bool) -> Result<(), String> {
        let mut pending = vec![root.to_path_buf()];
        let (mut total, mut package, mut cache, mut temporary) = (0u64, 0u64, 0u64, 0u64);
        let mut entries = 0usize;
        while let Some(path) = pending.pop() {
            super::deadline::check()?;
            if expired() {
                return Err("MCP installation duration quota exceeded".into());
            }
            entries += 1;
            if entries + pending.len() > self.entries {
                return Err("MCP installation entry quota exceeded".into());
            }
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(error) if disappeared(&error) => continue,
                Err(_) => return Err("Cannot inspect installation resource usage".into()),
            };
            if metadata.file_type().is_symlink() {
                return Err("Unsafe symbolic link in MCP staging".into());
            }
            if metadata.is_dir() {
                let directory = match std::fs::read_dir(&path) {
                    Ok(d) => d,
                    Err(error) if disappeared(&error) => continue,
                    Err(_) => return Err("Cannot inspect installation resource directory".into()),
                };
                for entry in directory {
                    match entry {
                        Ok(e) => pending.push(e.path()),
                        Err(error) if disappeared(&error) => {}
                        Err(_) => return Err("Cannot inspect installation resource entry".into()),
                    }
                    if entries + pending.len() > self.entries {
                        return Err("MCP installation entry quota exceeded".into());
                    }
                }
            } else if metadata.is_file() {
                if metadata.len() > self.file_bytes {
                    return Err("MCP installation single-file quota exceeded".into());
                }
                #[cfg(unix)]
                let size = {
                    use std::os::unix::fs::MetadataExt;
                    if metadata.nlink() != 1 {
                        return Err("Unsafe hard link in MCP staging".into());
                    }
                    metadata.len().max(metadata.blocks().saturating_mul(512))
                };
                #[cfg(not(unix))]
                let size = metadata.len();
                total = total.checked_add(size).ok_or("MCP quota size overflow")?;
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| "MCP quota escaped staging")?;
                if relative.starts_with("mcp") {
                    package = package.saturating_add(size);
                }
                if relative.starts_with("npm-environment/cache") {
                    cache = cache.saturating_add(size);
                }
                if relative.starts_with("npm-environment/tmp") {
                    temporary = temporary.saturating_add(size);
                }
                if total > self.total_bytes {
                    return Err("MCP total staging quota exceeded".into());
                }
                if package > self.package_bytes {
                    return Err("MCP extracted-package quota exceeded".into());
                }
                if cache > self.cache_bytes {
                    return Err("MCP npm-cache quota exceeded".into());
                }
                if temporary > self.temporary_bytes {
                    return Err("MCP temporary-file quota exceeded".into());
                }
            } else {
                return Err("Unexpected filesystem entry in MCP staging".into());
            }
        }
        Ok(())
    }
}

pub(super) async fn monitor<T>(
    attempt: &super::cleanup::StagingAttempt,
    limits: Limits,
    work: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    limits.check(attempt)?;
    tokio::pin!(work);
    loop {
        tokio::select! {
            result = &mut work => { limits.check(attempt)?; return result; },
            _ = tokio::time::sleep(limits.interval) => { limits.check(attempt)?; }
        }
    }
}

pub(super) fn configure(command: &mut tokio::process::Command) {
    #[cfg(target_os = "macos")]
    unsafe {
        command.pre_exec(|| {
            let limit = libc::rlimit {
                rlim_cur: 64 * MIB,
                rlim_max: 64 * MIB,
            };
            if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = command;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotas_count_cache_temporary_packages_files_and_total() {
        for (name, quota) in [
            ("npm-environment/cache/file", 0),
            ("npm-environment/tmp/file", 1),
            ("mcp/file", 2),
            ("other", 3),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let file = temp.path().join(name);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, b"too much").unwrap();
            let mut limits = MCP;
            match quota {
                0 => limits.cache_bytes = 1,
                1 => limits.temporary_bytes = 1,
                2 => limits.package_bytes = 1,
                _ => limits.total_bytes = 1,
            }
            assert!(limits.scan(temp.path(), || false).is_err());
            assert!(
                Limits { entries: 1, ..MCP }
                    .scan(temp.path(), || false)
                    .is_err()
            );
            assert!(
                Limits {
                    file_bytes: 1,
                    ..MCP
                }
                .scan(temp.path(), || false)
                .is_err()
            );
            assert!(
                MCP.scan(temp.path(), || true)
                    .unwrap_err()
                    .contains("duration")
            );
        }
    }
}
