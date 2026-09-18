//! Serial executor and timeouts for SDK identity / Remote Config / load-screen
//! calls.
//!
//! [`show_screen`](crate::show_screen) stays fire-and-present and does **not**
//! go through this queue.

use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use crate::error::QonversionError;

/// Default wait for a queued SDK call before returning [`QonversionError::Timeout`].
///
/// Timing out does **not** cancel native work — the serial worker still finishes
/// the in-flight call before starting the next one. Apps choose fail-open vs
/// fail-closed when they see a timeout.
pub const DEFAULT_SDK_TIMEOUT: Duration = Duration::from_secs(8);

static SDK_TIMEOUT: Mutex<Duration> = Mutex::new(DEFAULT_SDK_TIMEOUT);

struct Job {
    work: Box<dyn FnOnce() + Send + 'static>,
}

static JOB_TX: OnceLock<mpsc::Sender<Job>> = OnceLock::new();

/// Current timeout for queued SDK calls.
pub fn sdk_timeout() -> Duration {
    *SDK_TIMEOUT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Override the timeout for queued SDK calls (default [`DEFAULT_SDK_TIMEOUT`]).
pub fn set_sdk_timeout(timeout: Duration) {
    *SDK_TIMEOUT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = timeout;
}

fn job_sender() -> mpsc::Sender<Job> {
    JOB_TX
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel::<Job>();
            thread::Builder::new()
                .name("dioxus-qonversion-sdk".into())
                .spawn(move || {
                    while let Ok(job) = rx.recv() {
                        (job.work)();
                    }
                })
                .expect("failed to spawn dioxus-qonversion SDK worker");
            tx
        })
        .clone()
}

/// Run `work` on the single SDK worker thread.
///
/// Waits up to [`sdk_timeout`] for a result. On timeout returns
/// [`QonversionError::Timeout`] without cancelling `work`; the worker keeps
/// the slot until `work` returns so later queued calls do not overlap.
pub(crate) fn run_serial<T, F>(work: F) -> Result<T, QonversionError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, QonversionError> + Send + 'static,
{
    let timeout = sdk_timeout();
    let (reply_tx, reply_rx) = mpsc::channel();
    job_sender()
        .send(Job {
            work: Box::new(move || {
                let result = panic::catch_unwind(AssertUnwindSafe(work)).unwrap_or_else(|_| {
                    Err(QonversionError::Native {
                        message: "Qonversion SDK worker panicked".into(),
                    })
                });
                // Caller may have timed out and dropped the receiver —
                // that is intentional; native work still completed.
                let _ = reply_tx.send(result);
            }),
        })
        .map_err(|_| QonversionError::Native {
            message: "Qonversion SDK queue is closed".into(),
        })?;

    match reply_rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => Err(QonversionError::Timeout { timeout }),
        Err(RecvTimeoutError::Disconnected) => Err(QonversionError::Native {
            message: "Qonversion SDK worker disconnected before completing".into(),
        }),
    }
}

#[cfg(test)]
/// Serialize tests that share process-wide SDK state (worker, timeout, init flag).
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
struct RestoreSdkTimeout;

#[cfg(test)]
impl Drop for RestoreSdkTimeout {
    fn drop(&mut self) {
        set_sdk_timeout(DEFAULT_SDK_TIMEOUT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn timeout_returns_typed_error_and_work_still_finishes() {
        let _guard = test_lock();
        let _restore = RestoreSdkTimeout;
        set_sdk_timeout(Duration::from_millis(40));
        let finished = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&finished);

        let err = run_serial(move || {
            thread::sleep(Duration::from_millis(150));
            flag.store(true, Ordering::SeqCst);
            Ok(())
        })
        .expect_err("must time out");

        assert!(matches!(err, QonversionError::Timeout { .. }));

        // Native/work continues after the caller timed out.
        for _ in 0..50 {
            if finished.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            finished.load(Ordering::SeqCst),
            "timed-out work must still complete on the worker"
        );
    }

    #[test]
    fn jobs_do_not_overlap() {
        let _guard = test_lock();
        let _restore = RestoreSdkTimeout;
        set_sdk_timeout(Duration::from_secs(5));
        let concurrent = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));

        let spawn_job = |concurrent: Arc<AtomicUsize>, max_seen: Arc<AtomicUsize>| {
            thread::spawn(move || {
                run_serial(move || {
                    let now = concurrent.fetch_add(1, Ordering::SeqCst) + 1;
                    max_seen.fetch_max(now, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(60));
                    concurrent.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                })
            })
        };

        let a = spawn_job(Arc::clone(&concurrent), Arc::clone(&max_seen));
        let b = spawn_job(Arc::clone(&concurrent), Arc::clone(&max_seen));
        a.join().unwrap().expect("job a");
        b.join().unwrap().expect("job b");

        assert_eq!(max_seen.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn run_serial_returns_value() {
        let _guard = test_lock();
        let _restore = RestoreSdkTimeout;
        set_sdk_timeout(Duration::from_secs(5));
        let value = run_serial(|| Ok(42)).expect("value");
        assert_eq!(value, 42);
    }
}
