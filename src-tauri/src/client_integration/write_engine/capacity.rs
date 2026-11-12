//! Metadata-only accounting. Names count resources, never establish ownership.
use super::{Code, Result, fs::Dir};
use crate::client_integration::consent_models::{RecoveryState, RecoveryUsage};
use std::path::Path;
pub(super) const MAX_ARTIFACTS: u64 = 32;
const MAX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_FILES: u64 = 4096;
const RESERVE_BYTES: u64 = 12 * 1024 * 1024;
const RESERVE_FILES: u64 = 8;
const MIN_FREE: u64 = 64 * 1024 * 1024;
fn base() -> RecoveryUsage {
    RecoveryUsage {
        state: RecoveryState::Unknown,
        artifact_count: None,
        storage_bytes: None,
        file_count: None,
        available_disk_bytes: None,
        max_artifacts: MAX_ARTIFACTS,
        max_storage_bytes: MAX_BYTES,
        max_files: MAX_FILES,
        reserved_transaction_bytes: RESERVE_BYTES,
        automatic_cleanup_enabled: false,
    }
}
fn scan(dir: &Dir, bytes: &mut u64, files: &mut u64, depth: u32) -> Result<()> {
    if depth > 8 {
        return Err(Code::Unsupported);
    }
    for name in dir.names(MAX_FILES as usize)? {
        *files = files.checked_add(1).ok_or(Code::Unsupported)?;
        if *files > MAX_FILES {
            return Err(Code::Unsupported);
        }
        let (size, directory) = dir.private_entry(&name)?;
        if directory {
            scan(&dir.existing_child(&name)?, bytes, files, depth + 1)?;
        } else {
            *bytes = bytes.checked_add(size).ok_or(Code::Unsupported)?;
        }
    }
    Ok(())
}
pub(super) fn inspect(home: &Path, data: &Path) -> RecoveryUsage {
    let mut report = base();
    let work = || -> Result<(u64, u64, u64, u64)> {
        let data_dir = Dir::open(data)?;
        let free = data_dir.available_bytes()?;
        let mut artifacts = 0;
        let mut bytes = 0;
        let mut files = 0;
        match data_dir.existing_child("client-config-transactions") {
            Ok(root) => {
                root.private()?;
                for name in root.names(MAX_FILES as usize)? {
                    let (size, directory) = root.private_entry(&name)?;
                    if directory {
                        artifacts += 1;
                        scan(&root.existing_child(&name)?, &mut bytes, &mut files, 0)?;
                    } else if ["claude.lock", "codex.lock", "capacity.lock"]
                        .contains(&name.as_str())
                    {
                        if size != 0 {
                            return Err(Code::Unsupported);
                        }
                        files += 1;
                    } else {
                        return Err(Code::Unsupported);
                    }
                }
            }
            Err(Code::Missing) => {}
            Err(e) => return Err(e),
        }
        for parent in [home.to_path_buf(), home.join(".codex")] {
            let dir = match Dir::open(&parent) {
                Ok(dir) => dir,
                Err(Code::Missing) => continue,
                Err(e) => return Err(e),
            };
            for name in dir.names(MAX_FILES as usize)? {
                if name.starts_with(".figcon-") && name.ends_with(".candidate") {
                    let (size, directory) = dir.private_entry(&name)?;
                    if directory {
                        return Err(Code::Unsupported);
                    }
                    artifacts += 1;
                    files += 1;
                    bytes = bytes.checked_add(size).ok_or(Code::Unsupported)?;
                }
            }
        }
        Ok((artifacts, bytes, files, free))
    };
    if let Ok((count, bytes, files, free)) = work() {
        report.artifact_count = Some(count);
        report.storage_bytes = Some(bytes);
        report.file_count = Some(files);
        report.available_disk_bytes = Some(free);
        report.state = if count >= MAX_ARTIFACTS
            || bytes.saturating_add(RESERVE_BYTES) > MAX_BYTES
            || files.saturating_add(RESERVE_FILES) > MAX_FILES
            || free < MIN_FREE + RESERVE_BYTES
        {
            RecoveryState::Full
        } else {
            RecoveryState::Available
        };
    }
    report
}
pub(super) fn admit(home: &Path, data: &Path) -> Result<()> {
    if inspect(home, data).state != RecoveryState::Available {
        Err(Code::Capacity)
    } else {
        Ok(())
    }
}
