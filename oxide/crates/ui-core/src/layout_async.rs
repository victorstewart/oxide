//! Asynchronous layout coordinator for off-thread measurement and reflow.

#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc;
#[cfg(not(target_arch = "wasm32"))]
use std::thread::{self, JoinHandle};

#[cfg(not(target_arch = "wasm32"))]
enum Command<T>
where
    T: Send + 'static,
{
    Compute(Task<T>),
    Shutdown,
}

#[cfg(not(target_arch = "wasm32"))]
struct Task<T>
where
    T: Send + 'static,
{
    seq: u64,
    job: Box<dyn FnOnce() -> T + Send>,
}

struct TaskResult<T>
where
    T: Send + 'static,
{
    seq: u64,
    value: T,
}

/// Executes layout jobs on a background thread and returns the latest result.
///
/// Jobs are coalesced: when multiple requests are queued, only the newest
/// result is surfaced to the caller. Outdated results are dropped.
pub struct AsyncLayoutCoordinator<T>
where
    T: Send + 'static,
{
    #[cfg(not(target_arch = "wasm32"))]
    tx: mpsc::Sender<Command<T>>,
    #[cfg(not(target_arch = "wasm32"))]
    rx: mpsc::Receiver<TaskResult<T>>,
    #[cfg(not(target_arch = "wasm32"))]
    worker: Option<JoinHandle<()>>,
    #[cfg(target_arch = "wasm32")]
    pending: Option<TaskResult<T>>,
    next_seq: u64,
    last_requested: u64,
    last_applied: u64,
}

impl<T> AsyncLayoutCoordinator<T>
where
    T: Send + 'static,
{
    pub fn new() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            return Self {
                pending: None,
                next_seq: 1,
                last_requested: 0,
                last_applied: 0,
            };
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command<T>>();
        let (res_tx, res_rx) = mpsc::channel::<TaskResult<T>>();
        let worker = thread::Builder::new()
            .name(String::from("oxide-layout-worker"))
            .spawn(move || {
                while let Ok(cmd) = cmd_rx.recv() {
                    match cmd {
                        Command::Compute(task) => {
                            let Task { seq, job } = task;
                            let value = job();
                            if res_tx.send(TaskResult { seq, value }).is_err() {
                                break;
                            }
                        }
                        Command::Shutdown => break,
                    }
                }
            })
            .expect("spawn layout worker");

        Self {
            tx: cmd_tx,
            rx: res_rx,
            worker: Some(worker),
            next_seq: 1,
            last_requested: 0,
            last_applied: 0,
        }
        }
    }

    /// Queue a new layout job. Returns the sequence identifier associated
    /// with this computation.
    pub fn request<F>(&mut self, job: F) -> u64
    where
        F: FnOnce() -> T + Send + 'static,
    {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        self.last_requested = seq;
        #[cfg(target_arch = "wasm32")]
        {
            self.pending = Some(TaskResult { seq, value: job() });
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
        let task = Task { seq, job: Box::new(job) };
        let _ = self.tx.send(Command::Compute(task));
        }
        seq
    }

    /// Drain any completed jobs and return the most recent result.
    pub fn poll_latest(&mut self) -> Option<(u64, T)> {
        #[cfg(target_arch = "wasm32")]
        {
            let res = self.pending.take()?;
            if res.seq <= self.last_applied {
                return None;
            }
            self.last_applied = res.seq;
            return Some((res.seq, res.value));
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
        let mut latest: Option<TaskResult<T>> = None;
        while let Ok(res) = self.rx.try_recv() {
            if latest.as_ref().map_or(true, |cur| res.seq >= cur.seq) {
                latest = Some(res);
            }
        }
        if let Some(res) = latest {
            if res.seq <= self.last_applied {
                return None;
            }
            self.last_applied = res.seq;
            Some((res.seq, res.value))
        } else {
            None
        }
        }
    }

    pub fn has_inflight(&self) -> bool {
        self.last_applied < self.last_requested
    }

    fn shutdown(&mut self) {
        #[cfg(target_arch = "wasm32")]
        {
            self.pending = None;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
        if self.worker.is_none() {
            return;
        }
        let _ = self.tx.send(Command::Shutdown);
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
        }
    }
}

impl<T> Drop for AsyncLayoutCoordinator<T>
where
    T: Send + 'static,
{
    fn drop(&mut self) {
        self.shutdown();
    }
}
