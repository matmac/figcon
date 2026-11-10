use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::{
    fs::{MetadataExt, OpenOptionsExt},
    io::AsRawFd,
};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::{InstallationKind, Lifecycle, Record};
use crate::installer::attempt_inventory::{Identity, seal};
use crate::installer::paths::InstallerPaths;

pub(in crate::installer) fn validate_private(path: &Path) -> Result<fs::Metadata, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| "Cannot inspect private attempt directory")?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o777 != 0o700
    {
        return Err("Attempt directory ownership or permissions are unsafe".into());
    }
    Ok(metadata)
}

pub(in crate::installer) fn sync_directory(path: &Path) -> Result<(), String> {
    File::options()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Cannot sync installation directory".into())
}

fn nonce() -> Result<String, String> {
    let mut random = [0u8; 16];
    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .map_err(|_| "Cannot allocate installation attempt identity")?;
    Ok(random.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub(in crate::installer) fn load(path: &Path) -> Result<Record, String> {
    let bytes = crate::installer::mcp_fs::read_regular(path, 16 * 1024)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| "Cannot inspect attempt record")?;
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o777 != 0o600 {
        return Err("Unsafe attempt record ownership".into());
    }
    let record: Record = serde_json::from_slice(&bytes).map_err(|_| "Corrupt attempt record")?;
    if record.schema != 1
        || record.attempt_id.len() != 32
        || !record.attempt_id.bytes().all(|b| b.is_ascii_hexdigit())
        || record.seal.len() != 64
        || !record.seal.bytes().all(|b| b.is_ascii_hexdigit())
        || record.created_unix_ms == 0
        || record.directory.is_empty()
        || record.process.pid == 0
        || record.process.pid > i32::MAX as u32
        || record.process.start_seconds == 0
        || record.process.boot_session.len() != 36
        || !record
            .process
            .boot_session
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b'-')
        || !Path::new(&record.directory)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
        || Path::new(&record.directory).components().count() != 1
    {
        return Err("Unsupported attempt record".into());
    }
    Ok(record)
}

pub(in crate::installer) fn open_lease(path: &Path) -> Result<Option<File>, String> {
    let file = File::options()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "Missing or unsafe attempt lease")?;
    let metadata = file
        .metadata()
        .map_err(|_| "Cannot inspect attempt lease")?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o777 != 0o600
    {
        return Err("Unsafe attempt lease".into());
    }
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return if std::io::Error::last_os_error().raw_os_error() == Some(libc::EWOULDBLOCK) {
            Ok(None)
        } else {
            Err("Cannot establish attempt lease ownership".into())
        };
    }
    Ok(Some(file))
}

pub(in crate::installer) struct Journal {
    pub record: Mutex<Record>,
    pub record_path: PathBuf,
    pub lease_path: PathBuf,
    pub lease: File,
    guard: crate::installer::cleanup::DirectoryGuard,
    retention_reported: bool,
}

impl Journal {
    pub fn create(
        paths: &InstallerPaths,
        directory: &Path,
        kind: InstallationKind,
    ) -> Result<Self, String> {
        if paths.metadata != paths.root.join("metadata") {
            return Err("Attempt ledger escaped managed root".into());
        }
        let root = validate_private(&paths.root)?;
        let staging = validate_private(&paths.staging)?;
        let entry = validate_private(directory)?;
        if fs::read_dir(directory)
            .map_err(|_| "Cannot inspect new attempt")?
            .next()
            .is_some()
        {
            return Err("Cannot adopt a nonempty staging directory".into());
        }
        crate::installer::attempt_inventory::private_directory(&paths.metadata)?;
        let ledger = paths.metadata.join("attempts");
        crate::installer::attempt_inventory::private_directory(&ledger)?;
        let name = directory
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("Invalid attempt directory")?;
        let lease_path = ledger.join(format!("{name}.lease"));
        let file = File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&lease_path)
            .map_err(|_| "Cannot allocate attempt lease; staging retained")?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("Cannot acquire new attempt lease; staging retained".into());
        }
        file.sync_all().map_err(|_| "Cannot sync attempt lease")?;
        let record = Record {
            schema: 1,
            attempt_id: nonce()?,
            directory: name.to_owned(),
            kind,
            created_unix_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| "Invalid installation clock")?
                .as_millis()
                .try_into()
                .map_err(|_| "Invalid installation clock")?,
            process: crate::installer::attempt_process::ProcessIdentity::current()?,
            state: Lifecycle::Created,
            root: Identity::of(&root),
            staging: Identity::of(&staging),
            directory_identity: Identity::of(&entry),
            lease_identity: Identity::of(&file.metadata().map_err(|_| "Cannot inspect new lease")?),
            seal: seal(directory, kind)?,
        };
        let journal = Self {
            record: Mutex::new(record),
            record_path: ledger.join(format!("{name}.json")),
            lease_path,
            lease: file,
            guard: crate::installer::cleanup::DirectoryGuard::capture(&[
                &paths.root,
                &paths.staging,
                directory,
                &paths.metadata,
                &ledger,
            ])
            .map_err(|_| "Attempt directory identity changed")?,
            retention_reported: false,
        };
        journal.persist(true)?;
        sync_directory(&paths.staging)?;
        Ok(journal)
    }

    fn persist(&self, initial: bool) -> Result<(), String> {
        self.guard
            .verify()
            .map_err(|_| "Attempt or ledger identity changed")?;
        let record = self
            .record
            .lock()
            .map_err(|_| "Attempt journal is unavailable")?;
        let parent = self.record_path.parent().ok_or("Invalid ledger path")?;
        let temporary = parent.join(format!("{}.{}.tmp", record.attempt_id, nonce()?));
        let mut file = File::options()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temporary)
            .map_err(|_| "Cannot allocate atomic attempt record")?;
        let bytes = serde_json::to_vec(&*record).map_err(|_| "Cannot encode attempt record")?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| "Cannot persist attempt record; ledger temporary retained")?;
        if initial {
            fs::hard_link(&temporary, &self.record_path)
                .map_err(|_| "Attempt record already exists; ledger temporary retained")?;
            fs::remove_file(&temporary).map_err(|_| "Cannot remove owned ledger temporary")?;
        } else {
            fs::rename(&temporary, &self.record_path)
                .map_err(|_| "Cannot activate attempt record; ledger temporary retained")?;
        }
        sync_directory(parent)
    }

    pub fn verify(&self) -> Result<(), String> {
        self.guard
            .verify()
            .map_err(|_| "Attempt or ledger identity changed")?;
        let expected = self
            .record
            .lock()
            .map_err(|_| "Attempt journal is unavailable")?
            .clone();
        if load(&self.record_path)? != expected {
            return Err("Attempt metadata was externally modified".into());
        }
        let ledger = self.record_path.parent().ok_or("Invalid attempt ledger")?;
        let metadata = ledger.parent().ok_or("Invalid attempt metadata root")?;
        let root = metadata.parent().ok_or("Invalid managed root")?;
        validate_private(metadata)?;
        validate_private(ledger)?;
        for (path, identity) in [
            (root.to_path_buf(), expected.root),
            (root.join("staging"), expected.staging),
            (
                root.join("staging").join(&expected.directory),
                expected.directory_identity,
            ),
        ] {
            if Identity::of(&validate_private(&path)?) != identity {
                return Err("Attempt directory ownership changed".into());
            }
        }
        let lease =
            fs::symlink_metadata(&self.lease_path).map_err(|_| "Attempt lease disappeared")?;
        if Identity::of(&lease) != expected.lease_identity
            || Identity::of(&lease)
                != Identity::of(
                    &self
                        .lease
                        .metadata()
                        .map_err(|_| "Cannot inspect held lease")?,
                )
        {
            return Err("Attempt lease identity changed".into());
        }
        Ok(())
    }

    pub fn state(&self, state: Lifecycle) -> Result<(), String> {
        self.verify()?;
        self.record
            .lock()
            .map_err(|_| "Attempt journal is unavailable")?
            .state = state;
        self.persist(false)
    }

    pub fn checkpoint(&self, path: &Path, state: Lifecycle) -> Result<(), String> {
        self.verify()?;
        let kind = self
            .record
            .lock()
            .map_err(|_| "Attempt journal is unavailable")?
            .kind;
        let seal = seal(path, kind)?;
        crate::installer::deadline::check()?;
        {
            let mut record = self
                .record
                .lock()
                .map_err(|_| "Attempt journal is unavailable")?;
            record.state = state;
            record.seal = seal;
        }
        self.persist(false)
    }

    pub fn validate_seal(&self, path: &Path) -> Result<(), String> {
        self.verify()?;
        let record = self
            .record
            .lock()
            .map_err(|_| "Attempt journal is unavailable")?;
        if seal(path, record.kind)? != record.seal {
            return Err(
                "Staging contents changed since the durable checkpoint; manual review required"
                    .into(),
            );
        }
        Ok(())
    }

    /// Keep the receipt, lease path and payload intact; closing the live handle
    /// releases only its advisory lease. Do not re-seal or adopt changed data.
    pub fn retain(&mut self) -> Result<(), String> {
        self.verify()?;
        self.retention_reported = true;
        Ok(())
    }
}

impl Drop for Journal {
    fn drop(&mut self) {
        if self.retention_reported {
            return;
        }
        let prepared = self
            .record
            .lock()
            .map(|r| r.state == Lifecycle::Prepared)
            .unwrap_or(false);
        if prepared && self.state(Lifecycle::Cancelled).is_err() {
            eprintln!(
                "Cannot persist cancelled installation state; attempt retained for manual review"
            );
        }
        eprintln!(
            "Installation attempt retained; recovery will verify ownership, liveness, and its durable seal"
        );
    }
}
