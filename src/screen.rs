//! Present a Qonversion No-Codes screen and forward No-Codes events.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use serde_json::Value;

use crate::error::QonversionError;
use crate::init;
use crate::native;

struct HandlerSlot<T> {
    inner: Mutex<Option<Arc<dyn Fn(T) + Send + Sync>>>,
}

impl<T: Send + 'static> HandlerSlot<T> {
    const fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    fn set(&self, handler: impl Fn(T) + Send + Sync + 'static) {
        *self.lock() = Some(Arc::new(handler));
    }

    fn clear(&self) {
        *self.lock() = None;
    }

    fn dispatch(&self, value: T) {
        let handler = self.lock().clone();
        if let Some(handler) = handler {
            let _ = panic::catch_unwind(AssertUnwindSafe(|| handler(value)));
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Arc<dyn Fn(T) + Send + Sync>>> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

static SCREEN_FAILED_HANDLER: HandlerSlot<QonversionError> = HandlerSlot::new();
static SCREEN_EVENT_HANDLER: HandlerSlot<ScreenEvent> = HandlerSlot::new();

enum NotifyJob {
    Failed(QonversionError),
    EventJson(String),
}

fn notify_sender() -> mpsc::Sender<NotifyJob> {
    static TX: OnceLock<mpsc::Sender<NotifyJob>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<NotifyJob>();
        thread::Builder::new()
            .name("dioxus-qonversion-events".into())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    match job {
                        NotifyJob::Failed(error) => dispatch_screen_failed(error),
                        NotifyJob::EventJson(json) => match parse_event_envelope(&json) {
                            Ok(event) => dispatch_screen_event(event),
                            Err(err) => {
                                eprintln!("dioxus-qonversion: malformed screen event: {err}");
                                dispatch_screen_failed(err);
                            }
                        },
                    }
                }
            })
            .expect("failed to spawn dioxus-qonversion events worker");
        tx
    })
    .clone()
}

/// Kind of No-Codes action reported by the native SDK.
///
/// Navigation / URL / deeplink actions arrive as [`Self::Unknown`] — the SDK
/// already performs them; this crate has no follow-up API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenActionKind {
    Purchase,
    Restore,
    Close,
    CloseAll,
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
    let context_key = crate::helpers::require_non_empty("context_key", context_key)?;
    init::require_initialized()?;
    native::show_screen(context_key)
}

/// Register a process-wide handler for No-Codes screen load failures.
///
/// Called when a screen fails to load **after** present (for example Play
/// Billing product fetch), and when a native event envelope is malformed.
/// Replacing an existing handler is allowed. Register once from Dioxus `App`.
/// Delivery is off the native main thread; hop to the Dioxus scheduler if the
/// closure updates UI. Queued APIs (`identify`, `remote_config`, …) are safe
/// to call from this handler.
pub fn set_screen_failed_handler(handler: impl Fn(QonversionError) + Send + Sync + 'static) {
    SCREEN_FAILED_HANDLER.set(handler);
}

/// Clear the handler registered by [`set_screen_failed_handler`].
pub fn clear_screen_failed_handler() {
    SCREEN_FAILED_HANDLER.clear();
}

/// Register a process-wide handler for No-Codes purchase / restore / finish /
/// custom-action events.
///
/// Match [`ScreenEvent::ActionFinished`] with [`ScreenActionKind::Purchase`]
/// (or [`ScreenActionKind::Restore`]) to refresh status after a buy. Match
/// [`ScreenEvent::Finished`] for dismiss — it is **not** a purchase.
///
/// Replacing an existing handler is allowed. Register once from Dioxus `App`.
/// Delivery is off the native main thread; hop to the Dioxus scheduler if the
/// closure updates UI. Queued APIs are safe to call from this handler. Load
/// failures stay on [`set_screen_failed_handler`].
pub fn set_screen_event_handler(handler: impl Fn(ScreenEvent) + Send + Sync + 'static) {
    SCREEN_EVENT_HANDLER.set(handler);
}

/// Clear the handler registered by [`set_screen_event_handler`].
pub fn clear_screen_event_handler() {
    SCREEN_EVENT_HANDLER.clear();
}

/// Invoke the registered handler, if any. Panics in the handler are swallowed.
pub(crate) fn dispatch_screen_failed(error: QonversionError) {
    SCREEN_FAILED_HANDLER.dispatch(error);
}

pub(crate) fn dispatch_screen_event(event: ScreenEvent) {
    SCREEN_EVENT_HANDLER.dispatch(event);
}

pub(crate) fn hop_screen_failed(error: QonversionError) {
    let _ = notify_sender().send(NotifyJob::Failed(error));
}

pub(crate) fn hop_screen_event_json(json: String) {
    let _ = notify_sender().send(NotifyJob::EventJson(json));
}

pub(crate) fn parse_event_envelope(json: &str) -> Result<ScreenEvent, QonversionError> {
    let value: Value = serde_json::from_str(json).map_err(|err| QonversionError::Native {
        message: format!("invalid screen event JSON: {err}"),
    })?;
    let object = value.as_object().ok_or_else(|| QonversionError::Native {
        message: "screen event must be a JSON object".into(),
    })?;
    let kind =
        object
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| QonversionError::Native {
                message: "screen event missing kind".into(),
            })?;
    match kind {
        "action_finished" => Ok(ScreenEvent::ActionFinished {
            kind: parse_action_kind(object.get("action").and_then(Value::as_str)),
        }),
        "action_failed" => Ok(ScreenEvent::ActionFailed {
            kind: parse_action_kind(object.get("action").and_then(Value::as_str)),
            message: object
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        "finished" => Ok(ScreenEvent::Finished),
        "custom_action" => Ok(ScreenEvent::CustomAction {
            value: object
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        other => Err(QonversionError::Native {
            message: format!("unknown screen event kind: {other}"),
        }),
    }
}

fn parse_action_kind(raw: Option<&str>) -> ScreenActionKind {
    let raw = raw.unwrap_or("");
    if raw.eq_ignore_ascii_case("purchase") || raw.eq_ignore_ascii_case("makePurchase") {
        ScreenActionKind::Purchase
    } else if raw.eq_ignore_ascii_case("restore") {
        ScreenActionKind::Restore
    } else if raw.eq_ignore_ascii_case("close") {
        ScreenActionKind::Close
    } else if raw.eq_ignore_ascii_case("closeAll") || raw.eq_ignore_ascii_case("close_all") {
        ScreenActionKind::CloseAll
    } else {
        ScreenActionKind::Unknown
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
    hop_screen_failed(screen_failed_error(store_unavailable != 0, message));
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
    hop_screen_event_json(json);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;

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
        let (tx, rx) = mpsc::channel();
        set_screen_failed_handler(move |err| {
            let _ = tx.send(err);
        });

        dioxus_qonversion_notify_screen_failed(1, std::ptr::null());
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).expect("store"),
            QonversionError::StoreUnavailable
        );

        let msg = std::ffi::CString::new("backend down").unwrap();
        dioxus_qonversion_notify_screen_failed(0, msg.as_ptr());
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).expect("native"),
            QonversionError::Native {
                message: "backend down".into()
            }
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
            Ok(ScreenEvent::ActionFinished {
                kind: ScreenActionKind::Purchase
            })
        );
        assert_eq!(
            parse_event_envelope(r#"{"kind":"action_finished","action":"makePurchase"}"#),
            Ok(ScreenEvent::ActionFinished {
                kind: ScreenActionKind::Purchase
            })
        );
        assert_eq!(
            parse_event_envelope(r#"{"kind":"action_finished","action":"restore"}"#),
            Ok(ScreenEvent::ActionFinished {
                kind: ScreenActionKind::Restore
            })
        );
        assert_eq!(
            parse_event_envelope(r#"{"kind":"action_finished","action":"closeAll"}"#),
            Ok(ScreenEvent::ActionFinished {
                kind: ScreenActionKind::CloseAll
            })
        );
        assert_eq!(
            parse_event_envelope(r#"{"kind":"action_finished","action":"mystery"}"#),
            Ok(ScreenEvent::ActionFinished {
                kind: ScreenActionKind::Unknown
            })
        );
        assert_eq!(
            parse_event_envelope(r#"{"kind":"action_finished","action":"navigation"}"#),
            Ok(ScreenEvent::ActionFinished {
                kind: ScreenActionKind::Unknown
            })
        );
        assert_eq!(
            parse_event_envelope(r#"{"kind":"action_finished","action":"url"}"#),
            Ok(ScreenEvent::ActionFinished {
                kind: ScreenActionKind::Unknown
            })
        );
        assert_eq!(
            parse_event_envelope(r#"{"kind":"action_finished","action":"deeplink"}"#),
            Ok(ScreenEvent::ActionFinished {
                kind: ScreenActionKind::Unknown
            })
        );
        assert_eq!(
            parse_event_envelope(
                r#"{"kind":"action_failed","action":"purchase","message":"declined"}"#
            ),
            Ok(ScreenEvent::ActionFailed {
                kind: ScreenActionKind::Purchase,
                message: "declined".into()
            })
        );
        assert_eq!(
            parse_event_envelope(r#"{"kind":"finished"}"#),
            Ok(ScreenEvent::Finished)
        );
        assert_eq!(
            parse_event_envelope(r#"{"kind":"custom_action","value":"open_help"}"#),
            Ok(ScreenEvent::CustomAction {
                value: "open_help".into()
            })
        );
        assert!(parse_event_envelope(r#"{"kind":"nope"}"#).is_err());
        assert!(parse_event_envelope("not-json").is_err());
        assert!(parse_event_envelope("[]").is_err());
    }

    #[test]
    fn c_abi_dispatches_screen_event() {
        let _guard = queue::test_lock();
        let _restore = restore_handler();
        let (tx, rx) = mpsc::channel();
        set_screen_event_handler(move |event| {
            let _ = tx.send(event);
        });

        let json =
            std::ffi::CString::new(r#"{"kind":"action_finished","action":"purchase"}"#).unwrap();
        dioxus_qonversion_notify_screen_event(json.as_ptr());
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).expect("event"),
            ScreenEvent::ActionFinished {
                kind: ScreenActionKind::Purchase
            }
        );
    }

    #[test]
    fn c_abi_malformed_event_fires_failed_handler() {
        let _guard = queue::test_lock();
        let _restore = restore_handler();
        let (tx, rx) = mpsc::channel();
        set_screen_failed_handler(move |err| {
            let _ = tx.send(err);
        });

        let json = std::ffi::CString::new("not-json").unwrap();
        dioxus_qonversion_notify_screen_event(json.as_ptr());
        let err = rx.recv_timeout(Duration::from_secs(1)).expect("failed");
        assert!(matches!(err, QonversionError::Native { .. }));
    }
}
