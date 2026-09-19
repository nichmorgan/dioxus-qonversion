//! On-demand No-Codes `loadScreen`.

use serde_json::Value;

use crate::error::QonversionError;
use crate::init;
use crate::native;
use crate::queue;

/// Kind of a [`ScreenVariable`] from the No-Codes Builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenVariableKind {
    Custom,
    Product,
    SelectedProduct,
    Unknown,
}

/// Typed default configured on a No-Codes screen.
#[derive(Debug, Clone, PartialEq)]
pub enum ScreenVariableValue {
    Bool(bool),
    String(String),
    Number(f64),
    None,
}

/// One default variable or product slot from official `loadScreen`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenVariable {
    pub kind: ScreenVariableKind,
    pub key: String,
    /// Configured type string: `"boolean"`, `"string"`, or `"number"`.
    pub value_type: String,
    pub value: ScreenVariableValue,
}

/// Screen identity and builder defaults returned by a successful [`load_screen`].
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedScreen {
    pub id: String,
    pub context_key: String,
    /// Builder Default Product, when configured.
    pub default_selected_product_id: Option<String>,
    /// Typed custom variables and product slots. Empty when none are configured.
    pub default_variables: Vec<ScreenVariable>,
}

impl LoadedScreen {
    /// First variable with this key, optionally filtered by kind.
    ///
    /// Keys are only unique within a kind. Prefer
    /// [`Self::default_selected_product_id`] for the screen default product.
    pub fn default_variable(
        &self,
        key: &str,
        kind: Option<ScreenVariableKind>,
    ) -> Option<&ScreenVariable> {
        self.default_variables.iter().find(|variable| {
            variable.key == key
                && kind
                    .map(|expected| variable.kind == expected)
                    .unwrap_or(true)
        })
    }
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
    let default_selected_product_id = object
        .get("default_selected_product_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string);
    let default_variables = match object.get("default_variables") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(parse_screen_variable)
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => {
            return Err(QonversionError::Native {
                message: "load screen default_variables must be an array".into(),
            });
        }
    };

    Ok(LoadedScreen {
        id,
        context_key,
        default_selected_product_id,
        default_variables,
    })
}

fn parse_screen_variable(value: &Value) -> Result<ScreenVariable, QonversionError> {
    let object = value.as_object().ok_or_else(|| QonversionError::Native {
        message: "screen variable must be a JSON object".into(),
    })?;
    let key = object
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| QonversionError::Native {
            message: "screen variable missing key".into(),
        })?
        .to_string();
    let value_type = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Ok(ScreenVariable {
        kind: parse_variable_kind(object.get("kind").and_then(Value::as_str)),
        key,
        value_type,
        value: parse_variable_value(object.get("value")),
    })
}

fn parse_variable_kind(raw: Option<&str>) -> ScreenVariableKind {
    match raw.unwrap_or("") {
        "custom" => ScreenVariableKind::Custom,
        "product" => ScreenVariableKind::Product,
        "selected_product" => ScreenVariableKind::SelectedProduct,
        _ => ScreenVariableKind::Unknown,
    }
}

fn parse_variable_value(value: Option<&Value>) -> ScreenVariableValue {
    match value {
        None | Some(Value::Null) => ScreenVariableValue::None,
        Some(Value::Bool(flag)) => ScreenVariableValue::Bool(*flag),
        Some(Value::String(text)) => ScreenVariableValue::String(text.clone()),
        Some(Value::Number(number)) => number
            .as_f64()
            .map(ScreenVariableValue::Number)
            .unwrap_or(ScreenVariableValue::None),
        Some(_) => ScreenVariableValue::None,
    }
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
        assert!(screen.default_selected_product_id.is_none());
        assert!(screen.default_variables.is_empty());
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

    #[test]
    fn default_variables_and_selected_product() {
        let screen = parse_envelope(
            r#"{
                "ok": true,
                "id": "scr_1",
                "context_key": "paywall",
                "default_selected_product_id": "yearly",
                "default_variables": [
                    {"kind":"custom","key":"show_trial","type":"boolean","value":true},
                    {"kind":"product","key":"primary","type":"string","value":"monthly"},
                    {"kind":"selected_product","key":"default_selected_product","type":"string","value":"yearly"},
                    {"kind":"custom","key":"price","type":"number","value":9.99}
                ]
            }"#,
            "paywall",
        )
        .expect("vars");
        assert_eq!(
            screen.default_selected_product_id.as_deref(),
            Some("yearly")
        );
        assert_eq!(screen.default_variables.len(), 4);
        let show_trial = screen
            .default_variable("show_trial", Some(ScreenVariableKind::Custom))
            .expect("show_trial");
        assert_eq!(show_trial.value, ScreenVariableValue::Bool(true));
        let primary = screen
            .default_variable("primary", Some(ScreenVariableKind::Product))
            .expect("primary");
        assert_eq!(primary.value, ScreenVariableValue::String("monthly".into()));
        assert_eq!(
            screen.default_variable("price", None).map(|v| &v.value),
            Some(&ScreenVariableValue::Number(9.99))
        );
    }
}
