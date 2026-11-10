//! Recovery never executes commands or sends signals to processes.
use super::paths::InstallerPaths;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RetentionReason {
    Live,
    UnknownOwnership,
    UnknownLiveness,
    Unsealed,
    Modified,
    CleanupFailed,
    ManualReview,
}

#[derive(Debug, Serialize)]
pub struct RetainedAttempt {
    pub attempt_id: Option<String>,
    pub reason: RetentionReason,
}

#[derive(Debug, Default, Serialize)]
pub struct RecoveryReport {
    pub removed: Vec<String>,
    pub retained: Vec<RetainedAttempt>,
    /// Counts only; ledger names and untrusted contents are never logged.
    pub orphaned_ledger_files: usize,
}

pub fn recover_installations(paths: &InstallerPaths) -> Result<RecoveryReport, String> {
    #[cfg(target_os = "macos")]
    {
        macos::recover(paths)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = paths;
        Err("Installation recovery currently requires macOS process identity and leases".into())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use crate::installer::{
        attempt::{Lifecycle, load, open_lease, validate_private},
        attempt_inventory::{Identity, seal},
        attempt_process::Liveness,
        cleanup::DirectoryGuard,
    };
    use std::{fs, path::Path};

    fn missing(path: &Path) -> Result<bool, String> {
        match fs::symlink_metadata(path) {
            Ok(_) => Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
            Err(_) => Err("Cannot inspect recovery root".into()),
        }
    }

    pub(super) fn recover(paths: &InstallerPaths) -> Result<RecoveryReport, String> {
        if !paths.root.is_absolute()
            || paths.staging != paths.root.join("staging")
            || paths.metadata != paths.root.join("metadata")
            || paths.root.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
        {
            return Err("Recovery paths must stay inside the managed installation root".into());
        }
        let mut report = RecoveryReport::default();
        if missing(&paths.root)? {
            return Ok(report);
        }
        let root = validate_private(&paths.root)?;
        if missing(&paths.staging)? {
            report_orphans(paths, &mut report)?;
            return Ok(report);
        }
        let staging = validate_private(&paths.staging)?;
        let guard = DirectoryGuard::capture(&[&paths.root, &paths.staging])
            .map_err(|_| "Unsafe recovery roots")?;
        for entry in fs::read_dir(&paths.staging).map_err(|_| "Cannot enumerate managed staging")? {
            crate::installer::deadline::check()?;
            if report.removed.len() + report.retained.len() >= 128 {
                return Err("Recovery candidate limit exceeded; staging preserved".into());
            }
            guard
                .verify()
                .map_err(|_| "Recovery root identity changed")?;
            let entry = entry.map_err(|_| "Cannot inspect recovery candidate")?;
            let result = recover_one(
                paths,
                &entry.path(),
                &Identity::of(&root),
                &Identity::of(&staging),
            );
            match result {
                Ok(id) => report.removed.push(id),
                Err((id, reason)) => report.retained.push(RetainedAttempt {
                    attempt_id: id,
                    reason,
                }),
            }
        }
        report_orphans(paths, &mut report)?;
        Ok(report)
    }

    fn report_orphans(paths: &InstallerPaths, report: &mut RecoveryReport) -> Result<(), String> {
        let ledger = paths.metadata.join("attempts");
        if !missing(&ledger)? {
            validate_private(&paths.metadata)?;
            validate_private(&ledger)?;
            let ledger_guard = DirectoryGuard::capture(&[&paths.root, &paths.metadata, &ledger])
                .map_err(|_| "Unsafe orphan ledger roots")?;
            let handle = ledger_guard
                .handle(&ledger)
                .map_err(|_| "Cannot hold ledger")?;
            let mut count = 0;
            for name in crate::installer::anchored::names(&handle)
                .map_err(|_| "Cannot enumerate orphan ledger")?
            {
                count += 1;
                if count > 1024 {
                    return Err("Recovery ledger limit exceeded; manual review required".into());
                }
                let known = name
                    .to_str()
                    .and_then(|n| n.strip_suffix(".json").or_else(|| n.strip_suffix(".lease")));
                if known.is_none_or(|n| fs::symlink_metadata(paths.staging.join(n)).is_err()) {
                    report.orphaned_ledger_files += 1;
                }
            }
            ledger_guard
                .verify()
                .map_err(|_| "Orphan ledger identity changed")?;
        }
        Ok(())
    }

    fn recover_one(
        paths: &InstallerPaths,
        path: &Path,
        root: &Identity,
        staging: &Identity,
    ) -> Result<String, (Option<String>, RetentionReason)> {
        let unknown = || (None, RetentionReason::UnknownOwnership);
        let metadata = validate_private(path).map_err(|_| unknown())?;
        let ledger = paths.metadata.join("attempts");
        validate_private(&paths.metadata).map_err(|_| unknown())?;
        validate_private(&ledger).map_err(|_| unknown())?;
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(unknown)?;
        let record_path = ledger.join(format!("{name}.json"));
        let lease_path = ledger.join(format!("{name}.lease"));
        let guard =
            DirectoryGuard::capture(&[&paths.root, &paths.staging, path, &paths.metadata, &ledger])
                .map_err(|_| unknown())?;
        let before = load(&record_path).map_err(|_| unknown())?;
        let id = Some(before.attempt_id.clone());
        let retain = |reason| (id.clone(), reason);
        let lease = open_lease(&lease_path)
            .map_err(|_| retain(RetentionReason::UnknownOwnership))?
            .ok_or_else(|| retain(RetentionReason::Live))?;
        let record = load(&record_path).map_err(|_| retain(RetentionReason::UnknownOwnership))?;
        if record != before
            || record.directory != name
            || record.root != *root
            || record.staging != *staging
            || record.directory_identity != Identity::of(&metadata)
            || record.lease_identity
                != Identity::of(
                    &lease
                        .metadata()
                        .map_err(|_| retain(RetentionReason::UnknownOwnership))?,
                )
        {
            return Err(retain(RetentionReason::UnknownOwnership));
        }
        match record.process.liveness() {
            Liveness::Live => return Err(retain(RetentionReason::Live)),
            Liveness::Unknown => return Err(retain(RetentionReason::UnknownLiveness)),
            Liveness::Dead => {}
        }
        // A process can outlive its parent after a crash during a command. No
        // quiescent checkpoint means no automatic deletion, even for a dead PID.
        if record.state == Lifecycle::Running {
            return Err(retain(RetentionReason::Unsealed));
        }
        if seal(path, record.kind).map_err(|_| retain(RetentionReason::Modified))? != record.seal {
            return Err(retain(RetentionReason::Modified));
        }
        guard
            .verify()
            .map_err(|_| retain(RetentionReason::Modified))?;
        if load(&record_path).map_err(|_| retain(RetentionReason::Modified))? != record {
            return Err(retain(RetentionReason::Modified));
        }
        // Recheck birth identity immediately before deletion; recovery never
        // kills or adopts a process, and never follows a staging symlink.
        if record.process.liveness() != Liveness::Dead {
            return Err(retain(RetentionReason::UnknownLiveness));
        }
        // No automatic deletion until recovery can persist a complete entry
        // manifest and bind it to descriptor-relative traversal. A hash alone
        // is not deletion authority for an abandoned tree.
        Err(retain(RetentionReason::ManualReview))
    }
}
