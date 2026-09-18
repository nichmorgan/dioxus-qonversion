//! Present a Qonversion No-Codes screen and forward No-Codes events.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::error::QonversionError;
use crate::init;
use crate::native;

type ScreenFailedHandler = dyn Fn(QonversionError) + Send + Sync;
type ScreenEventHandler = dyn Fn(ScreenEvent) + Send + Sync;

static SCREEN_FAILED_HANDLER: Mutex<Option<Arc<ScreenFailedHandler>>> = Mutex::new(None);
static SCREEN_EVENT_HANDLER: Mutex<Option<Arc<ScreenEventHandler>>> = Mutex::new(None);

fn failed_handler_lock() -> std::sync::MutexGuard<'static, Option<Arc<ScreenFailedHandler>>> {
    SCREEN_FAILED_HANDLER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn event_handler_lock() -> std::sync::MutexGuard<'static, Option<Arc<ScreenEventHandler>>> {
    SCREEN_EVENT_HANDLER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Kind of No-Codes action reported by the native SDK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenActionKind {
    Purchase,
    Restore,
    Close,
    CloseAll,
    Navigation,
    Url,
    Deeplink,
    Unknown,
}

/// Process-wide No-Codes event after a screen is presented.
///
/// [`Self::ActionFinished`] with [`ScreenActionKind::Purchase`] is the buy
/// signal. [`Self::Finished`] means the flow closed — do not treat it as a
/// purchase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenEvent {
    ActionFinished {
        kind: ScreenActionKind,
    },
    ActionFailed {
        kind: ScreenActionKind,
        message: String,
    },
    Finished,
    CustomAction {
        value: String,
    },
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
    *failed_handler_lock() = Some(Arc::new(handler));
}

/// Clear the handler registered by [`set_screen_failed_handler`].
pub fn clear_screen_failed_handler() {
    *failed_handler_lock() = None;
}

/// Register a process-wide handler for No-Codes purchase / restore / finish /
/// custom-action events.
///
/// Match [`ScreenEvent::ActionFinished`] with [`ScreenActionKind::Purchase`]
/// (or [`ScreenActionKind::Restore`]) to refresh status after a buy. Match
/// [`ScreenEvent::Finished`] for dismiss — it is **not** a purchase.
///
/// Replacing an existing handler is allowed. Register once from Dioxus `App`;
/// hop to the Dioxus scheduler if the closure updates UI (it may run on the
/// native main thread). Load failures stay on [`set_screen_failed_handler`].
pub fn set_screen_event_handler(handler: impl Fn(ScreenEvent) + Send + Sync + 'static) {
    *event_handler_lock() = Some(Arc::new(handler));
}

/// Clear the handler registered by [`set_screen_event_handler`].
pub fn clear_screen_event_handler() {
    *event_handler_lock() = None;
}

/// Invoke the registered handler, if any. Panics in the handler are swallowed.
pub(crate) fn dispatch_screen_failed(error: QonversionError) {
    let handler = failed_handler_lock().clone();
    if let Some(handler) = handler {
        let _ = panic::catch_unwind(AssertUnwindSafe(|| handler(error)));
    }
}

pub(crate) fn dispatch_screen_event(event: ScreenEvent) {
    let handler = event_handler_lock().clone();
    if let Some(handler) = handler {
        let _ = panic::catch_unwind(AssertUnwindSafe(|| handler(event)));
    }
}

pub(crate) fn parse_event_envelope(json: &str) -> Option<ScreenEvent> {
    let value: Value = serde_json::from_str(json).ok()?;
    let object = value.as_object()?;
    let kind = object.get("kind")?.as_str()?;
    match kind {
        "action_finished" => Some(ScreenEvent::ActionFinished {
            kind: parse_action_kind(object.get("action").and_then(Value::as_str)),
        }),
        "action_failed" => Some(ScreenEvent::ActionFailed {
            kind: parse_action_kind(object.get("action").and_then(Value::as_str)),
            message: object
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        "finished" => Some(ScreenEvent::Finished),
        "custom_action" => Some(ScreenEvent::CustomAction {
            value: object
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        _ => None,
    }
}

fn parse_action_kind(raw: Option<&str>) -> ScreenActionKind {
    match raw.unwrap_or("").to_ascii_lowercase().as_str() {
        "purchase" | "makepurchase" => ScreenActionKind::Purchase,
        "restore" => ScreenActionKind::Restore,
        "close" => ScreenActionKind::Close,
        "closeall" | "close_all" => ScreenActionKind::CloseAll,
        "navigation" => ScreenActionKind::Navigation,
        "url" => ScreenActionKind::Url,
        "deeplink" => ScreenActionKind::Deeplink,
        _ => ScreenActionKind::Unknown,
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

/// Called from the iOS Swift host (and tests) with a JSON screen-event envelope.
#[no_mangle]
pub extern "C" fn dioxus_qonversion_notify_screen_event(json: *const c_char) {
    if json.is_null() {
        return;
    }
    let json = unsafe { CStr::from_ptr(json) }
        .to_string_lossy()
        .into_owned();
    if let Some(event) = parse_event_envelope(&json) {
        dispatch_screen_event(event);
    }
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
                clear_screen_event_handler();
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

    #[test]
    fn event_handler_set_dispatch_replace_and_clear() {
        let _guard = queue::test_lock();
        let _restore = restore_handler();
        clear_screen_event_handler();

        let hits = Arc::new(AtomicUsize::new(0));
        let last = Arc::new(StdMutex::new(None::<ScreenEvent>));

        let hits_a = Arc::clone(&hits);
        let last_a = Arc::clone(&last);
        set_screen_event_handler(move |event| {
            hits_a.fetch_add(1, Ordering::SeqCst);
            *last_a.lock().unwrap() = Some(event);
        });

        dispatch_screen_event(ScreenEvent::ActionFinished {
            kind: ScreenActionKind::Purchase,
        });
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(
            last.lock().unwrap().clone(),
            Some(ScreenEvent::ActionFinished {
                kind: ScreenActionKind::Purchase
            })
        );

        let hits_b = Arc::clone(&hits);
        set_screen_event_handler(move |_| {
            hits_b.fetch_add(10, Ordering::SeqCst);
        });
        dispatch_screen_event(ScreenEvent::Finished);
        assert_eq!(hits.load(Ordering::SeqCst), 11);

        clear_screen_event_handler();
        dispatch_screen_event(ScreenEvent::Finished);
        assert_eq!(hits.load(Ordering::SeqCst), 11);
    }

    #[test]
    fn parse_event_kinds() {
        assert_eq!(
            parse_event_envelope(r#"{"kind":"action_finished","action":"purchase"}"#),
            Some(ScreenEvent::ActionFinished {
                kind: ScreenActionKind::Purchase
            })
        );
        assert_eq!(
            parse_event_envelope(r#"{"kind":"action_finished","action":"makePurchase"}"#),
            Some(ScreenEvent::ActionFinished {
                kind: ScreenActionKind::Purchase
            })
        );
        assert_eq!(
            parse_event_envelope(r#"{"kind":"action_finished","action":"restore"}"#),
            Some(ScreenEvent::ActionFinished {
                kind: ScreenActionKind::Restore
            })
        );
        assert_eq!(
            parse_event_envelope(r#"{"kind":"action_finished","action":"closeAll"}"#),
            Some(ScreenEvent::ActionFinished {
                kind: ScreenActionKind::CloseAll
            })
        );
        assert_eq!(
            parse_event_envelope(r#"{"kind":"action_finished","action":"mystery"}"#),
            Some(ScreenEvent::ActionFinished {
                kind: ScreenActionKind::Unknown
            })
        );
        assert_eq!(
            parse_event_envelope(
                r#"{"kind":"action_failed","action":"purchase","message":"declined"}"#
            ),
            Some(ScreenEvent::ActionFailed {
                kind: ScreenActionKind::Purchase,
                message: "declined".into()
            })
        );
        assert_eq!(
            parse_event_envelope(r#"{"kind":"finished"}"#),
            Some(ScreenEvent::Finished)
        );
        assert_eq!(
            parse_event_envelope(r#"{"kind":"custom_action","value":"open_help"}"#),
            Some(ScreenEvent::CustomAction {
                value: "open_help".into()
            })
        );
        assert_eq!(parse_event_envelope(r#"{"kind":"nope"}"#), None);
    }

    #[test]
    fn c_abi_dispatches_screen_event() {
        let _guard = queue::test_lock();
        let _restore = restore_handler();
        let last = Arc::new(StdMutex::new(None::<ScreenEvent>));
        let last_h = Arc::clone(&last);
        set_screen_event_handler(move |event| {
            *last_h.lock().unwrap() = Some(event);
        });

        let json =
            std::ffi::CString::new(r#"{"kind":"action_finished","action":"purchase"}"#).unwrap();
        dioxus_qonversion_notify_screen_event(json.as_ptr());
        assert_eq!(
            last.lock().unwrap().clone(),
            Some(ScreenEvent::ActionFinished {
                kind: ScreenActionKind::Purchase
            })
        );
    }
}
