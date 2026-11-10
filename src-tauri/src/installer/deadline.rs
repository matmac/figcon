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
    groups: std::sync::Mutex<Vec<std::sync::Weak<super::process::Group>>>,
}
thread_local! { static CURRENT: RefCell<Option<Deadline>> = const { RefCell::new(None) }; }
impl Deadline {
    pub fn new(duration: Duration) -> Self {
        Self {
            end: Instant::now() + duration,
            state: Arc::new(State {
                cancelled: AtomicBool::new(false),
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
        self.0.state.cancelled.store(true, Ordering::Release);
        if let Ok(groups) = self.0.state.groups.lock() {
            for group in groups.iter().filter_map(std::sync::Weak::upgrade) {
                if !group.stop() {
                    eprintln!("Cannot terminate cancelled installation group; staging retained");
                }
            }
        }
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
    run_with(Deadline::current(), work).await
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
        .map_err(|_| "Installation worker failed; staging retained")?;
    // A staged handle inherits the deadline; successful return is not cancellation.
    cancellation.0 = Deadline::new(Duration::from_secs(0));
    result
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
