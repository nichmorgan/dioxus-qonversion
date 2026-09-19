//! Shared crate-internal helpers.

use serde_json::{Map, Value};

use crate::error::QonversionError;

/// Native string APIs (`identify`, `logout`) return this when the timed wait expires.
#[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(dead_code))]
pub(crate) const HOST_TIMEOUT_SENTINEL: &str = "dioxus_qonversion:timeout";

#[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(dead_code))]
pub(crate) fn map_host_error_message(message: String) -> QonversionError {
    if message == HOST_TIMEOUT_SENTINEL {
        QonversionError::Timeout {
            timeout: crate::queue::sdk_timeout(),
        }
    } else {
        QonversionError::Native { message }
    }
}

pub(crate) fn require_non_empty<'a>(label: &str, s: &'a str) -> Result<&'a str, QonversionError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(QonversionError::InvalidConfig(format!(
            "{label} must not be empty"
        )));
    }
    Ok(s)
}

/// Parse a native `{ok: bool, ...}` JSON object.
///
/// On `ok: false`, `on_false` may return a more specific error (for example
/// [`QonversionError::ScreenNotFound`]); otherwise this becomes
/// [`QonversionError::Native`] from the `error` field.
pub(crate) fn parse_ok_envelope(
    json: &str,
    label: &str,
    on_false: impl FnOnce(&Map<String, Value>) -> Option<QonversionError>,
) -> Result<Map<String, Value>, QonversionError> {
    let value: Value = serde_json::from_str(json).map_err(|err| QonversionError::Native {
        message: format!("invalid {label} envelope: {err}"),
    })?;
    let object = match value {
        Value::Object(map) => map,
        _ => {
            return Err(QonversionError::Native {
                message: format!("{label} envelope must be a JSON object"),
            });
        }
    };

    match object.get("ok") {
        Some(Value::Bool(true)) => Ok(object),
        Some(Value::Bool(false)) => {
            if object
                .get("timed_out")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return Err(QonversionError::Timeout {
                    timeout: crate::queue::sdk_timeout(),
                });
            }
            if let Some(err) = on_false(&object) {
                return Err(err);
            }
            let message = object
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown native error");
            Err(QonversionError::Native {
                message: message.to_string(),
            })
        }
        _ => Err(QonversionError::Native {
            message: format!("{label} envelope missing ok:true"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timed_out_flag_is_timeout() {
        let err = parse_ok_envelope(
            r#"{"ok":false,"timed_out":true,"error":"late"}"#,
            "test",
            |_| None,
        )
        .expect_err("timed out");
        assert!(matches!(err, QonversionError::Timeout { .. }));
    }

    #[test]
    fn host_sentinel_is_timeout() {
        let err = map_host_error_message(HOST_TIMEOUT_SENTINEL.into());
        assert!(matches!(err, QonversionError::Timeout { .. }));
    }

    #[test]
    fn host_other_message_is_native() {
        assert_eq!(
            map_host_error_message("offline".into()),
            QonversionError::Native {
                message: "offline".into()
            }
        );
    }
}
