//! One cooperative deadline, shared by blocking installation work and its caller.
use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Clone)]
pub(super) struct Deadline {
    end: Instant,
    state: Arc<State>,
}
struct State {
    cancelled: AtomicBool,
    requested: AtomicBool,
    committed: std::sync::Mutex<bool>,
    observer: Option<Arc<super::progress::Progress>>,
    groups: std::sync::Mutex<Vec<std::sync::Weak<super::process::Group>>>,
}
thread_local! { static CURRENT: RefCell<Option<Deadline>> = const { RefCell::new(None) }; }
impl Deadline {
    pub fn new(duration: Duration) -> Self {
        Self::observed(duration, None)
    }
    pub fn observed(duration: Duration, observer: Option<Arc<super::progress::Progress>>) -> Self {
        Self {
            end: Instant::now() + duration,
            state: Arc::new(State {
                cancelled: AtomicBool::new(false),
                requested: AtomicBool::new(false),
                committed: std::sync::Mutex::new(false),
                observer,
                groups: std::sync::Mutex::new(Vec::new()),
            }),
        }
    }
    pub fn current() -> Self {
        CURRENT
            .with(|c| c.borrow().clone())
            .unwrap_or_else(|| Self::new(Duration::from_secs(600)))
    }
    pub fn check(&self) -> Result<(), String> {
        if self.state.cancelled.load(Ordering::Acquire) {
            return Err("Installation cancelled; staging retained".into());
        }
        if Instant::now() >= self.end {
            return Err("Installation deadline exceeded; staging retained".into());
        }
        Ok(())
    }
    pub fn is_cancelled(&self) -> bool {
        self.state.requested.load(Ordering::Acquire)
    }
    pub fn cancel(&self) -> super::api_models::CancellationDisposition {
        self.stop(true)
    }
    fn stop(&self, requested: bool) -> super::api_models::CancellationDisposition {
        use super::api_models::CancellationDisposition::*;
        let Ok(committed) = self.state.committed.lock() else {
            return TooLate;
        };
        if *committed {
            return TooLate;
        }
        let already = self.state.requested.load(Ordering::Acquire);
        if requested {
            self.state.requested.store(true, Ordering::Release);
        }
        self.state.cancelled.store(true, Ordering::Release);
        if let Ok(groups) = self.state.groups.lock() {
            for group in groups.iter().filter_map(std::sync::Weak::upgrade) {
                if !group.stop() {
                    eprintln!("Cannot terminate cancelled installation group; staging retained");
                }
            }
        }
        if already { AlreadyRequested } else { Requested }
    }
    pub fn enter(&self) -> Scope {
        Scope(CURRENT.with(|c| c.replace(Some(self.clone()))))
    }
}
pub(super) struct Scope(Option<Deadline>);
impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.with(|c| c.replace(self.0.take()));
    }
}
pub(super) fn check() -> Result<(), String> {
    CURRENT.with(|c| c.borrow().as_ref().map_or(Ok(()), Deadline::check))
}
pub(super) fn observer() -> Option<Arc<super::progress::Progress>> {
    CURRENT.with(|c| c.borrow().as_ref().and_then(|d| d.state.observer.clone()))
}
/// Cancellation and the no-replace rename share a linearization point.
#[cfg(target_os = "macos")]
pub(super) fn activate(work: impl FnOnce() -> std::io::Result<()>) -> std::io::Result<()> {
    let deadline = Deadline::current();
    let mut committed = deadline
        .state
        .committed
        .lock()
        .map_err(|_| std::io::Error::other("Activation state unavailable"))?;
    deadline.check().map_err(std::io::Error::other)?;
    work()?;
    *committed = true;
    drop(committed); // Never acquire the revision clock while holding activation.
    if let Some(observer) = &deadline.state.observer {
        observer.activated();
    }
    Ok(())
}
#[cfg(target_os = "macos")]
pub(super) fn register_group(group: &Arc<super::process::Group>) {
    CURRENT.with(|current| {
        if let Some(deadline) = current.borrow().as_ref() {
            if let Ok(mut groups) = deadline.state.groups.lock() {
                groups.retain(|g| g.strong_count() > 0);
                groups.push(Arc::downgrade(group));
                if deadline.state.cancelled.load(Ordering::Acquire) && !group.stop() {
                    eprintln!("Cannot terminate cancelled installer group; staging retained");
                }
            }
        }
    });
}
struct Cancel(Deadline);
impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.stop(false);
    }
}
static WORKERS: AtomicUsize = AtomicUsize::new(0);
struct Slot;
impl Drop for Slot {
    fn drop(&mut self) {
        WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}
/// At most 32 blocking workers. Dropping the caller requests cooperative
/// cancellation; a blocked kernel operation cannot be forcibly preempted.
pub(super) async fn run<T, F, Fut>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    // A current-thread worker already owns this deadline. Avoid detached nested
    // workers so admission remains held until cooperative work has settled.
    if CURRENT.with(|c| c.borrow().is_some()) {
        check()?;
        work().await
    } else {
        run_with(Deadline::current(), work).await
    }
}

pub(super) async fn run_with<T, F, Fut>(deadline: Deadline, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    deadline.check()?;
    if WORKERS
        .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < 32).then_some(n + 1)
        })
        .is_err()
    {
        return Err("Installation worker limit reached".into());
    }
    let mut cancellation = Cancel(deadline.clone());
    let slot = Slot;
    let task = tokio::task::spawn_blocking(move || {
        let _slot = slot;
        let _scope = deadline.enter();
        deadline.check()?;
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "Cannot initialize installation worker")?
            .block_on(async {
                let work = work();
                tokio::pin!(work);
                loop {
                    tokio::select! {
                        result = &mut work => return result,
                        _ = tokio::time::sleep(Duration::from_millis(50)) => { deadline.check()?; }
                    }
                }
            })
    });
    let result = task
        .await
        .map_err(|_| "Installation worker failed; staging retained");
    // A staged handle inherits the deadline; successful return is not cancellation.
    cancellation.0 = Deadline::new(Duration::from_secs(0));
    result?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn blocking_work_does_not_block_caller_and_drop_requests_cancellation() {
        let finished = Arc::new(AtomicBool::new(false));
        let worker_finished = finished.clone();
        let mut work = Box::pin(run(move || async move {
            loop {
                if check().is_err() {
                    worker_finished.store(true, Ordering::Release);
                    return Err::<(), _>("cancelled".into());
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }));
        tokio::select! { _=&mut work => panic!("Unexpected completion"), _=tokio::time::sleep(Duration::from_millis(20))=>{} }
        drop(work);
        for _ in 0..100 {
            if finished.load(Ordering::Acquire) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("Worker did not cooperate with cancellation");
    }
}
