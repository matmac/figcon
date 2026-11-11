//! In-memory IPC lifecycle; production filesystem admission remains independent.
use super::{
    api_models::*,
    deadline::Deadline,
    progress::{Progress, Revision, Sink},
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(super) struct Operation {
    pub owner: String,
    pub progress: Arc<Progress>,
    pub deadline: Deadline,
}
#[derive(Default)]
struct Inner {
    active: Option<Arc<Operation>>,
    last: [Option<OperationReport>; 2],
}
#[derive(Default)]
pub struct Service {
    inner: Mutex<Inner>,
    revision: Revision,
}
impl Service {
    pub(super) async fn execute(
        &self,
        operation: Arc<Operation>,
        paths: super::paths::InstallerPaths,
    ) {
        let component = operation.progress.component;
        let result = super::deadline::run_with(operation.deadline.clone(), move || async move {
            super::api_backend::execute(paths, component).await
        })
        .await;
        self.finish(&operation, result);
    }
    pub(super) fn reserve(
        &self,
        component: Component,
        owner: String,
        id: String,
        sink: Sink,
    ) -> Result<Arc<Operation>, InstallationError> {
        let mut revision = self
            .revision
            .lock()
            .map_err(|_| InstallationError::new(ErrorCode::StateUnavailable))?;
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| InstallationError::new(ErrorCode::StateUnavailable))?;
        if inner.active.is_some() {
            return Err(InstallationError::new(ErrorCode::Busy));
        }
        let progress = Arc::new(Progress::shared(id, component, sink, self.revision.clone()));
        let op = Arc::new(Operation {
            owner,
            deadline: Deadline::observed(Duration::from_secs(600), Some(progress.clone())),
            progress,
        });
        inner.active = Some(op.clone());
        *revision += 1;
        drop(inner);
        drop(revision);
        // Event callbacks may synchronously query state. No state lock is held.
        op.progress.milestone(Stage::Preparing);
        Ok(op)
    }
    pub(super) fn finish(&self, op: &Arc<Operation>, result: Result<(), String>) {
        let Ok(mut revision) = self.revision.lock() else {
            return;
        };
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if !inner.active.as_ref().is_some_and(|a| Arc::ptr_eq(a, op)) {
            return;
        }
        let (_, activated, hint, _) = op.progress.details();
        let code = if op.deadline.is_cancelled() && !activated {
            Some(ErrorCode::Cancelled)
        } else {
            result
                .as_ref()
                .err()
                .map(|e| hint.unwrap_or_else(|| map_error(e)))
        };
        let state = match code {
            None => OperationState::Installed,
            Some(ErrorCode::Cancelled) => OperationState::Cancelled,
            Some(ErrorCode::AttemptLimit | ErrorCode::Busy) => OperationState::Blocked,
            _ => OperationState::Failed,
        };
        let error = code.map(InstallationError::new);
        // Publish completion before delivery, so reentrant listeners can query
        // terminal history and new attempts can begin without waiting for callbacks.
        op.progress.terminal_at(&mut revision, state, error.clone());
        let (retained, activated, _, last_event) = op.progress.details();
        let report = OperationReport {
            cancellation_requested: op.deadline.is_cancelled(),
            attempt_id: op.progress.id.clone(),
            component: op.progress.component,
            state,
            target_version: op.progress.component.version().into(),
            retained_staging: retained,
            activated,
            error,
            last_event,
        };
        inner.last[op.progress.component.index()] = Some(report);
        inner.active = None;
        *revision += 1;
        drop(inner);
        drop(revision);
        op.progress.dispatch();
    }
    pub(super) fn cancel(
        &self,
        owner: &str,
        id: &str,
    ) -> Result<CancellationReceipt, InstallationError> {
        let mut revision = self
            .revision
            .lock()
            .map_err(|_| InstallationError::new(ErrorCode::StateUnavailable))?;
        let inner = self
            .inner
            .lock()
            .map_err(|_| InstallationError::new(ErrorCode::StateUnavailable))?;
        let op = inner
            .active
            .as_ref()
            .filter(|a| a.progress.id == id)
            .ok_or_else(|| InstallationError::new(ErrorCode::NotActive))?;
        if op.owner != owner {
            return Err(InstallationError::new(ErrorCode::NotOwner));
        }
        let disposition = op.deadline.cancel();
        if disposition == CancellationDisposition::Requested {
            *revision += 1;
        }
        Ok(CancellationReceipt {
            attempt_id: id.into(),
            disposition,
        })
    }
    pub(super) fn revision(&self) -> Result<u64, InstallationError> {
        self.revision
            .lock()
            .map(|r| *r)
            .map_err(|_| InstallationError::new(ErrorCode::StateUnavailable))
    }
    pub(super) async fn inspect<F, Fut>(
        &self,
        mut inspect: F,
    ) -> Result<ManagedInstallationReport, InstallationError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<ManagedInstallationReport, InstallationError>>,
    {
        for _ in 0..3 {
            let before = self.revision()?;
            let mut report = inspect().await?;
            let revision = self
                .revision
                .lock()
                .map_err(|_| InstallationError::new(ErrorCode::StateUnavailable))?;
            if before != *revision {
                continue;
            }
            self.merge(&mut report)?;
            report.revision = *revision;
            return Ok(report);
        }
        Err(InstallationError::new(ErrorCode::UnstableSnapshot))
    }
    #[cfg(test)]
    pub(super) fn snapshot(
        &self,
        report: &mut ManagedInstallationReport,
    ) -> Result<(), InstallationError> {
        let revision = self
            .revision
            .lock()
            .map_err(|_| InstallationError::new(ErrorCode::StateUnavailable))?;
        self.merge(report)?;
        report.revision = *revision;
        Ok(())
    }
    fn merge(&self, report: &mut ManagedInstallationReport) -> Result<(), InstallationError> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| InstallationError::new(ErrorCode::StateUnavailable))?;
        for target in [&mut report.node, &mut report.mcp] {
            target.last_operation = inner.last[target.component.index()].clone();
            if let Some(op) = inner
                .active
                .as_ref()
                .filter(|op| op.progress.component == target.component)
            {
                let (retained, activated, _, last_event) = op.progress.details();
                target.state = OperationState::Installing;
                target.last_operation = Some(OperationReport {
                    cancellation_requested: op.deadline.is_cancelled(),
                    attempt_id: op.progress.id.clone(),
                    component: target.component,
                    state: OperationState::Installing,
                    target_version: target.component.version().into(),
                    retained_staging: retained,
                    activated,
                    error: None,
                    last_event,
                });
            } else if target.state != OperationState::Installed
                && let Some(last) = &target.last_operation
                && matches!(
                    last.state,
                    OperationState::Failed | OperationState::Cancelled | OperationState::Blocked
                )
            {
                target.state = last.state;
                target.error = last.error.clone();
            }
        }
        if report.retention.attempt_count.is_some_and(|n| n >= 8) {
            for target in [&mut report.node, &mut report.mcp] {
                if target.state != OperationState::Installing
                    && target.disk_state != DiskState::Installed
                {
                    target.state = OperationState::Blocked;
                    target.error = Some(InstallationError::new(ErrorCode::AttemptLimit));
                }
            }
        }
        Ok(())
    }
}
// Internal errors remain private; only a closed set of static messages crosses IPC.
pub(super) fn map_error(error: &str) -> ErrorCode {
    let e = error.to_ascii_lowercase();
    if e.contains("api operation busy") {
        ErrorCode::Busy
    } else if e.contains("api destination exists") {
        ErrorCode::AlreadyInstalled
    } else if e.contains("api runtime required") {
        ErrorCode::RuntimeRequired
    } else if e.contains("deadline") || e.contains("timed out") || e.contains("timeout") {
        ErrorCode::DeadlineExceeded
    } else if e.contains("checksum") || e.contains("integrity") || e.contains("lockfile") {
        ErrorCode::IntegrityFailed
    } else if e.contains("limit exceeded")
        || e.contains("quota")
        || e.contains("worker limit")
        || e.contains("maximum allowed size")
    {
        ErrorCode::ResourceLimit
    } else if e.contains("validat") {
        ErrorCode::ValidationFailed
    } else {
        ErrorCode::OperationFailed
    }
}
