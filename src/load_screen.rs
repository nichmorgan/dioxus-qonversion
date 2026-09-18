//! On-demand No-Codes `loadScreen`.

use serde_json::Value;

use crate::error::QonversionError;
use crate::init;
use crate::native;
use crate::queue;

/// Screen identity returned by a successful [`load_screen`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedScreen {
    pub id: String,
    pub context_key: String,
}

/// Fetch a No-Codes screen by context key without presenting it.
///
/// Optional — [`crate::show_screen`] works without this. Runs on the serial
/// SDK queue with [`crate::DEFAULT_SDK_TIMEOUT`]. Blocking wait: call from
/// Dioxus `spawn` / a background thread; avoid the UI thread.
///
/// Official `loadScreen` does not flush pending user properties, so targeting
/// can differ slightly from [`crate::show_screen`].
///
/// [`QonversionError::ScreenNotFound`] means the dashboard has no screen for
/// this key. Other SDK failures are [`QonversionError::Native`] or
/// [`QonversionError::Timeout`].
pub fn load_screen(context_key: &str) -> Result<LoadedScreen, QonversionError> {
    let context_key = crate::helpers::require_non_empty("context_key", context_key)?.to_string();
    init::require_initialized()?;
    queue::run_serial(move || {
        let envelope = native::load_screen(&context_key)?;
        parse_envelope(&envelope, &context_key)
    })
}

/// Parse the native host JSON envelope into [`LoadedScreen`].
pub(crate) fn parse_envelope(
    json: &str,
    requested_key: &str,
) -> Result<LoadedScreen, QonversionError> {
    let object = crate::helpers::parse_ok_envelope(json, "load screen", |object| {
        object
            .get("screen_not_found")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            .then_some(QonversionError::ScreenNotFound)
    })?;

    let id = object
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| QonversionError::Native {
            message: "load screen envelope missing id".into(),
        })?
        .to_string();
    let context_key = object
        .get("context_key")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())
        .unwrap_or(requested_key)
        .to_string();

    Ok(LoadedScreen { id, context_key })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_id_is_error() {
        let err = parse_envelope(r#"{"ok":true,"id":"","context_key":"paywall"}"#, "paywall")
            .expect_err("empty id");
        assert!(matches!(err, QonversionError::Native { .. }));
    }

    #[test]
    fn missing_id_is_error() {
        let err = parse_envelope(r#"{"ok":true,"context_key":"paywall"}"#, "paywall")
            .expect_err("missing id");
        assert!(matches!(err, QonversionError::Native { .. }));
    }

    #[test]
    fn missing_context_key_uses_requested() {
        let screen =
            parse_envelope(r#"{"ok":true,"id":"scr_1"}"#, "paywall").expect("fallback key");
        assert_eq!(screen.id, "scr_1");
        assert_eq!(screen.context_key, "paywall");
    }

    #[test]
    fn screen_not_found_flag() {
        let err = parse_envelope(
            r#"{"ok":false,"error":"missing","screen_not_found":true}"#,
            "paywall",
        )
        .expect_err("not found");
        assert_eq!(err, QonversionError::ScreenNotFound);
    }

    #[test]
    fn native_error_without_flag() {
        let err =
            parse_envelope(r#"{"ok":false,"error":"offline"}"#, "paywall").expect_err("native");
        assert_eq!(
            err,
            QonversionError::Native {
                message: "offline".into()
            }
        );
    }

    #[test]
    fn missing_ok_is_error() {
        let err = parse_envelope(r#"{"id":"x"}"#, "paywall").expect_err("missing ok");
        assert!(matches!(err, QonversionError::Native { .. }));
    }
}
