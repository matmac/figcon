//! Attempt-local observer shared through the existing deadline's worker context.
use super::api_models::*;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
pub(super) type Revision = Arc<Mutex<u64>>;
use std::time::{Duration, Instant};
pub(super) const EVENT_NAME: &str = "figcon://installation-progress";
pub(super) type Sink = Arc<dyn Fn(InstallationEvent) + Send + Sync>;
struct Stream {
    sequence: u64,
    pending: VecDeque<InstallationEvent>,
    dispatching: bool,
    retained: bool,
    activated: bool,
    terminal: bool,
    last: Option<InstallationEvent>,
    last_download: Option<Instant>,
    hint: Option<ErrorCode>,
}
pub(super) struct Progress {
    pub id: String,
    pub component: Component,
    sink: Sink,
    stream: Mutex<Stream>,
    revision: Revision,
}
impl Progress {
    #[cfg(test)]
    pub fn new(id: String, component: Component, sink: Sink) -> Self {
        Self::shared(id, component, sink, Revision::default())
    }
    pub fn shared(id: String, component: Component, sink: Sink, revision: Revision) -> Self {
        Self {
            revision,
            id,
            component,
            sink,
            stream: Mutex::new(Stream {
                sequence: 0,
                pending: VecDeque::new(),
                dispatching: false,
                retained: false,
                activated: false,
                terminal: false,
                last: None,
                last_download: None,
                hint: None,
            }),
        }
    }
    fn emit(
        &self,
        stage: Stage,
        status: EventStatus,
        bytes: Option<u64>,
        total: Option<u64>,
        error: Option<InstallationError>,
        terminal: bool,
    ) {
        if let Ok(mut revision) = self.revision.lock() {
            self.enqueue(&mut revision, stage, status, bytes, total, error, terminal);
        }
        self.dispatch();
    }
    fn enqueue(
        &self,
        revision: &mut u64,
        stage: Stage,
        status: EventStatus,
        bytes: Option<u64>,
        total: Option<u64>,
        error: Option<InstallationError>,
        terminal: bool,
    ) {
        if let Ok(mut s) = self.stream.lock() {
            if s.terminal {
                return;
            }
            *revision = revision
                .checked_add(1)
                .expect("Installation revision exhausted");
            s.sequence += 1;
            s.terminal = terminal;
            let event = InstallationEvent {
                schema_version: 1,
                revision: *revision,
                attempt_id: self.id.clone(),
                component: self.component,
                sequence: s.sequence,
                stage,
                status,
                downloaded_bytes: bytes,
                total_bytes: total,
                retained_staging: s.retained,
                activated: s.activated,
                error,
            };
            s.last = Some(event.clone());
            s.pending.push_back(event);
        }
    }
    /// One drainer, no state locks across callbacks. Reentrant publishers enqueue
    /// and return; the original drainer delivers their events in sequence order.
    pub fn dispatch(&self) {
        {
            let Ok(mut s) = self.stream.lock() else {
                return;
            };
            if s.dispatching {
                return;
            }
            s.dispatching = true;
        }
        loop {
            let event = {
                let Ok(mut s) = self.stream.lock() else {
                    return;
                };
                match s.pending.pop_front() {
                    Some(e) => e,
                    None => {
                        s.dispatching = false;
                        return;
                    }
                }
            };
            (self.sink)(event);
        }
    }
    pub fn terminal_at(
        &self,
        revision: &mut u64,
        state: OperationState,
        error: Option<InstallationError>,
    ) {
        let (stage, status) = match state {
            OperationState::Installed => (Stage::Completed, EventStatus::Succeeded),
            OperationState::Cancelled => (Stage::Cancelled, EventStatus::Cancelled),
            _ => (Stage::Failed, EventStatus::Failed),
        };
        self.enqueue(revision, stage, status, None, None, error, true);
    }
    pub fn milestone(&self, stage: Stage) {
        self.emit(stage, EventStatus::Running, None, None, None, false);
    }
    pub fn download(&self, bytes: u64, total: Option<u64>, force: bool) {
        let send = if let Ok(mut s) = self.stream.lock() {
            if !force
                && s.last_download
                    .is_some_and(|t| t.elapsed() < Duration::from_millis(100))
            {
                false
            } else {
                s.last_download = Some(Instant::now());
                true
            }
        } else {
            false
        };
        if send {
            self.emit(
                Stage::Downloading,
                EventStatus::Running,
                Some(bytes),
                total,
                None,
                false,
            );
        }
    }
    pub fn staging(&self) {
        if let Ok(mut r) = self.revision.lock() {
            if let Ok(mut s) = self.stream.lock() {
                s.retained = true;
                *r += 1;
            }
        }
        self.milestone(Stage::Preparing);
    }
    pub fn activated(&self) {
        if let Ok(mut r) = self.revision.lock() {
            if let Ok(mut s) = self.stream.lock() {
                s.activated = true;
                *r += 1;
            }
        }
    }
    pub fn hint(&self, code: ErrorCode) {
        if let Ok(mut r) = self.revision.lock() {
            if let Ok(mut s) = self.stream.lock() {
                s.hint = Some(code);
                *r += 1;
            }
        }
    }
    pub fn details(&self) -> (bool, bool, Option<ErrorCode>, Option<InstallationEvent>) {
        self.stream
            .lock()
            .map(|s| (s.retained, s.activated, s.hint, s.last.clone()))
            .unwrap_or((true, false, Some(ErrorCode::OperationFailed), None))
    }
}
pub(super) fn current() -> Option<Arc<Progress>> {
    super::deadline::observer()
}
pub(super) fn attempt_id() -> Option<String> {
    current().map(|p| p.id.clone())
}
pub(super) fn milestone(stage: Stage) {
    if let Some(p) = current() {
        p.milestone(stage);
    }
}
pub(super) fn staging_created() {
    if let Some(p) = current() {
        p.staging();
    }
}
pub(super) fn downloaded(bytes: u64, total: Option<u64>, force: bool) {
    if let Some(p) = current() {
        p.download(bytes, total, force);
    }
}
pub(super) fn hint(code: ErrorCode) {
    if let Some(p) = current() {
        p.hint(code);
    }
}
