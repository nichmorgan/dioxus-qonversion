//! Shared crate-internal helpers.

use serde_json::{Map, Value};

use crate::error::QonversionError;

pub(crate) fn require_context_key(s: &str) -> Result<&str, QonversionError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(QonversionError::InvalidConfig(
            "context_key must not be empty".into(),
        ));
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
