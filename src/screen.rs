//! Present a Qonversion No-Codes screen.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

use crate::error::QonversionError;
use crate::init;
use crate::native;

type ScreenFailedHandler = dyn Fn(QonversionError) + Send + Sync;

static SCREEN_FAILED_HANDLER: Mutex<Option<Arc<ScreenFailedHandler>>> = Mutex::new(None);

fn handler_lock() -> std::sync::MutexGuard<'static, Option<Arc<ScreenFailedHandler>>> {
    SCREEN_FAILED_HANDLER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Present a No-Codes screen by context key.
///
/// Returns when the native SDK has been asked to show the screen, not when
/// the user dismisses it. A missing/invalid key may still return `Ok` and
/// fail later via [`set_screen_failed_handler`].
///
/// On Android, a BillingClient preflight may return
/// [`QonversionError::StoreUnavailable`] **before** the screen is presented.
/// That wait must not run on the main looper — call from Dioxus `spawn` / a
/// background thread to get the typed error without flashing UI. UI-thread
/// calls skip the blocking preflight (happy path unchanged) and rely on
/// [`set_screen_failed_handler`] if product fetch fails after present.
///
/// Requires a successful [`crate::initialize`] first. Desktop and web return
/// [`QonversionError::UnsupportedPlatform`].
pub fn show_screen(context_key: &str) -> Result<(), QonversionError> {
    if context_key.trim().is_empty() {
        return Err(QonversionError::InvalidConfig(
            "context_key must not be empty".into(),
        ));
    }
    init::require_initialized()?;
    native::show_screen(context_key)
}

/// Register a process-wide handler for No-Codes screen load failures.
///
/// Called when a screen fails to load **after** present (for example Play
/// Billing product fetch). Replacing an existing handler is allowed. Register
/// once from Dioxus `App`; hop to the Dioxus scheduler if the closure updates
/// UI (it may run on the native main thread).
pub fn set_screen_failed_handler(handler: impl Fn(QonversionError) + Send + Sync + 'static) {
    *handler_lock() = Some(Arc::new(handler));
}

/// Clear the handler registered by [`set_screen_failed_handler`].
pub fn clear_screen_failed_handler() {
    *handler_lock() = None;
}

/// Invoke the registered handler, if any. Panics in the handler are swallowed.
pub(crate) fn dispatch_screen_failed(error: QonversionError) {
    let handler = handler_lock().clone();
    if let Some(handler) = handler {
        let _ = panic::catch_unwind(AssertUnwindSafe(|| handler(error)));
    }
}

pub(crate) fn screen_failed_error(store_unavailable: bool, message: String) -> QonversionError {
    if store_unavailable {
        QonversionError::StoreUnavailable
    } else {
        QonversionError::Native { message }
    }
}

/// Called from the iOS Swift host (and tests) when a No-Codes screen fails to load.
///
/// `store_unavailable != 0` maps to [`QonversionError::StoreUnavailable`];
/// otherwise the UTF-8 `message` is forwarded as [`QonversionError::Native`].
#[no_mangle]
pub extern "C" fn dioxus_qonversion_notify_screen_failed(
    store_unavailable: i32,
    message: *const c_char,
) {
    let message = if message.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned()
    };
    dispatch_screen_failed(screen_failed_error(store_unavailable != 0, message));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex as StdMutex;

    fn restore_handler() -> impl Drop {
        struct Restore;
        impl Drop for Restore {
            fn drop(&mut self) {
                clear_screen_failed_handler();
            }
        }
        Restore
    }

    #[test]
    fn handler_set_dispatch_replace_and_clear() {
        let _guard = queue::test_lock();
        let _restore = restore_handler();
        clear_screen_failed_handler();

        let hits = Arc::new(AtomicUsize::new(0));
        let last = Arc::new(StdMutex::new(None::<QonversionError>));

        let hits_a = Arc::clone(&hits);
        let last_a = Arc::clone(&last);
        set_screen_failed_handler(move |err| {
            hits_a.fetch_add(1, Ordering::SeqCst);
            *last_a.lock().unwrap() = Some(err);
        });

        dispatch_screen_failed(QonversionError::StoreUnavailable);
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(
            last.lock().unwrap().clone(),
            Some(QonversionError::StoreUnavailable)
        );

        let hits_b = Arc::clone(&hits);
        set_screen_failed_handler(move |_| {
            hits_b.fetch_add(10, Ordering::SeqCst);
        });
        dispatch_screen_failed(QonversionError::Native {
            message: "other".into(),
        });
        assert_eq!(hits.load(Ordering::SeqCst), 11);

        clear_screen_failed_handler();
        dispatch_screen_failed(QonversionError::StoreUnavailable);
        assert_eq!(hits.load(Ordering::SeqCst), 11);
    }

    #[test]
    fn c_abi_maps_store_unavailable_and_native() {
        let _guard = queue::test_lock();
        let _restore = restore_handler();
        let last = Arc::new(StdMutex::new(None::<QonversionError>));
        let last_h = Arc::clone(&last);
        set_screen_failed_handler(move |err| {
            *last_h.lock().unwrap() = Some(err);
        });

        dioxus_qonversion_notify_screen_failed(1, std::ptr::null());
        assert_eq!(
            last.lock().unwrap().clone(),
            Some(QonversionError::StoreUnavailable)
        );

        let msg = std::ffi::CString::new("backend down").unwrap();
        dioxus_qonversion_notify_screen_failed(0, msg.as_ptr());
        assert_eq!(
            last.lock().unwrap().clone(),
            Some(QonversionError::Native {
                message: "backend down".into()
            })
        );
    }
}
