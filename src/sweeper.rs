use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Handle to an active background sweeper thread.
///
/// Dropping this handle will signal the sweeper thread to stop gracefully.
pub struct SweeperHandle {
    stop_signal: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
}

impl SweeperHandle {
    /// Creates and starts a new background sweeper with the provided tick closure.
    pub fn start<F>(interval: Duration, mut purge_fn: F) -> Self
    where
        F: FnMut() -> usize + Send + 'static,
    {
        let stop_signal = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop_signal);

        let thread_handle = thread::Builder::new()
            .name("aerocache-sweeper".to_string())
            .spawn(move || {
                while !thread_stop.load(Ordering::Relaxed) {
                    thread::sleep(interval);
                    if thread_stop.load(Ordering::Relaxed) {
                        break;
                    }
                    purge_fn();
                }
            })
            .ok();

        Self {
            stop_signal,
            thread_handle,
        }
    }

    /// Explicitly signals the sweeper thread to terminate and waits for it to join.
    pub fn stop(&mut self) {
        self.stop_signal.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for SweeperHandle {
    fn drop(&mut self) {
        self.stop();
    }
}
