//! Read-only seals for quiescent private attempts. Unknown entries fail closed.
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    pub device: u64,
    pub inode: u64,
    pub uid: u32,
    pub mode: u32,
}

#[cfg(unix)]
impl Identity {
    pub fn of(metadata: &fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            uid: metadata.uid(),
            mode: metadata.mode(),
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) fn private_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("Cannot create private attempt ledger".into()),
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|_| "Cannot inspect private attempt ledger")?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o777 != 0o700
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(
            "Attempt ledger must be a real private directory owned by the current user".into(),
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(super) fn inventory(
    root: &Path,
    kind: super::attempt::InstallationKind,
) -> Result<
    (
        String,
        std::collections::BTreeMap<std::path::PathBuf, Signature>,
    ),
    String,
> {
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let mut pending = vec![root.to_path_buf()];
    let mut rows = Vec::new();
    let mut bytes = 0u64;
    let uid = unsafe { libc::geteuid() };
    let device = fs::symlink_metadata(root)
        .map_err(|_| "Attempt is missing")?
        .dev();
    while let Some(path) = pending.pop() {
        super::deadline::check()?;
        if rows.len() + pending.len() > 200_000 {
            return Err("Attempt inventory exceeds entry limit".into());
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "Attempt inventory escaped staging")?;
        let name = relative
            .to_str()
            .ok_or("Unknown non-UTF-8 staging entry")?
            .to_owned();
        let before = fs::symlink_metadata(&path).map_err(|_| "Cannot inspect attempt inventory")?;
        if before.uid() != uid
            || before.dev() != device
            || (!before.file_type().is_symlink() && before.mode() & 0o022 != 0)
        {
            return Err("Attempt inventory ownership or permissions changed".into());
        }
        let mut digest = Sha256::new();
        if before.file_type().is_symlink() {
            if kind != super::attempt::InstallationKind::Node {
                return Err("Unknown staging symbolic link".into());
            }
            let alias_name = relative
                .strip_prefix("runtime")
                .map_err(|_| "Unknown staging symbolic link")?;
            let alias = super::runtime_aliases::APPROVED
                .iter()
                .find(|a| alias_name.as_os_str() == std::ffi::OsStr::new(a.name))
                .ok_or("Unknown staging symbolic link")?;
            let target =
                fs::read_link(&path).map_err(|_| "Cannot inspect staging symbolic link")?;
            if target.as_os_str() != std::ffi::OsStr::new(alias.link_target) {
                return Err("Staging symbolic link target changed".into());
            }
            super::mcp_fs::real_file(&root.join("runtime"), Path::new(alias.file_target))?;
            digest.update(alias.link_target.as_bytes());
        } else if before.is_dir() {
            for entry in fs::read_dir(&path).map_err(|_| "Cannot inventory staging directory")? {
                pending.push(entry.map_err(|_| "Cannot inventory staging entry")?.path());
                if pending.len() + rows.len() > 200_000 {
                    return Err("Attempt inventory exceeds entry limit".into());
                }
            }
        } else if before.is_file() && before.nlink() == 1 {
            bytes = bytes
                .checked_add(before.len())
                .ok_or("Attempt inventory size overflow")?;
            if bytes > 2 * 1024 * 1024 * 1024 {
                return Err("Attempt inventory exceeds size limit".into());
            }
            let mut file = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(&path)
                .map_err(|_| "Cannot read staging inventory")?;
            if Identity::of(
                &file
                    .metadata()
                    .map_err(|_| "Cannot inspect inventory handle")?,
            ) != Identity::of(&before)
            {
                return Err("Staging file identity changed".into());
            }
            let mut buffer = [0u8; 64 * 1024];
            let mut read = 0u64;
            loop {
                super::deadline::check()?;
                let count = file
                    .read(&mut buffer)
                    .map_err(|_| "Cannot hash staging file")?;
                if count == 0 {
                    break;
                }
                read += count as u64;
                if read > before.len() {
                    return Err("Staging file changed during inventory".into());
                }
                digest.update(&buffer[..count]);
            }
            if read != before.len() {
                return Err("Staging file changed during inventory".into());
            }
        } else {
            return Err("Unknown staging filesystem entry".into());
        }
        let after =
            fs::symlink_metadata(&path).map_err(|_| "Staging entry vanished during inventory")?;
        if signature(&before) != signature(&after) {
            return Err("Staging entry changed during inventory".into());
        }
        rows.push((name, signature(&after), format!("{:x}", digest.finalize())));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let entries = rows
        .iter()
        .map(|(name, signature, _)| (std::path::PathBuf::from(name), signature.clone()))
        .collect();
    let bytes = serde_json::to_vec(&rows).map_err(|_| "Cannot encode staging inventory")?;
    super::deadline::check()?;
    Ok((format!("{:x}", Sha256::digest(bytes)), entries))
}

#[cfg(target_os = "macos")]
pub(super) type Signature = (Identity, u64, i64, i64, i64, i64, u64);
#[cfg(target_os = "macos")]
pub(super) fn signature(m: &fs::Metadata) -> Signature {
    use std::os::unix::fs::MetadataExt;
    (
        Identity::of(m),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
        m.nlink(),
    )
}
#[cfg(target_os = "macos")]
pub(super) fn seal(root: &Path, kind: super::attempt::InstallationKind) -> Result<String, String> {
    inventory(root, kind).map(|(seal, _)| seal)
}
