//! Fetch Qonversion Remote Config by context key.

use serde_json::{Map, Value};

use crate::error::QonversionError;
use crate::init;
use crate::native;
use crate::queue;

/// Dashboard JSON plus assignment metadata for one Remote Config.
///
/// Field names inside [`payload`](RemoteConfig::payload) are app-owned. This
/// crate does not memoize configs — clear any app-side cache after
/// [`crate::logout`].
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteConfig {
    /// JSON object configured in the Qonversion dashboard.
    pub payload: Map<String, Value>,
    /// Where this payload came from (remote config vs experiment group).
    pub source: Option<RemoteConfigurationSource>,
    /// Experiment assignment, when the payload is from an A/B test.
    pub experiment: Option<Experiment>,
}

/// Source of a [`RemoteConfig`] payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteConfigurationSource {
    /// Experiment or remote config identifier.
    pub id: String,
    /// Experiment or remote config name.
    pub name: String,
    /// How the payload was assigned to this user.
    pub assignment_type: RemoteConfigurationAssignmentType,
    /// Control group, treatment group, or a standalone remote config.
    pub source_type: RemoteConfigurationSourceType,
    /// Context key on the config. `None` when the dashboard key is empty.
    pub context_key: Option<String>,
}

/// How a payload was assigned (targeting vs a manual pin).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteConfigurationAssignmentType {
    Auto,
    Manual,
    Unknown,
}

/// Kind of configuration that produced the payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteConfigurationSourceType {
    RemoteConfiguration,
    ExperimentControlGroup,
    ExperimentTreatmentGroup,
    Unknown,
}

/// Experiment the user was assigned to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Experiment {
    pub id: String,
    pub name: String,
    pub group: ExperimentGroup,
}

/// Experiment group the user was assigned to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExperimentGroup {
    pub id: String,
    pub name: String,
    pub group_type: ExperimentGroupType,
}

/// Control vs treatment (or unknown).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExperimentGroupType {
    Control,
    Treatment,
    Unknown,
}

/// Fetch Remote Config for a dashboard context key.
///
/// Runs on the serial SDK queue with [`crate::DEFAULT_SDK_TIMEOUT`] (see
/// [`crate::set_sdk_timeout`]). Blocking wait: safe to call from Dioxus `spawn`
/// / a background thread; avoid the UI thread.
///
/// An empty payload map is success. SDK failures are
/// [`QonversionError::Native`] or [`QonversionError::Timeout`].
pub fn remote_config(context_key: &str) -> Result<RemoteConfig, QonversionError> {
    let context_key = crate::helpers::require_context_key(context_key)?;
    fetch(Some(context_key.to_string()))
}

/// Fetch Remote Config for the dashboard empty context key.
///
/// Same queue, timeout, and threading rules as [`remote_config`].
pub fn remote_config_default() -> Result<RemoteConfig, QonversionError> {
    fetch(None)
}

fn fetch(context_key: Option<String>) -> Result<RemoteConfig, QonversionError> {
    init::require_initialized()?;
    queue::run_serial(move || {
        let envelope = native::remote_config(context_key.as_deref())?;
        parse_envelope(&envelope)
    })
}

/// Parse the native host JSON envelope into [`RemoteConfig`].
pub(crate) fn parse_envelope(json: &str) -> Result<RemoteConfig, QonversionError> {
    let object = crate::helpers::parse_ok_envelope(json, "remote config", |_| None)?;

    let payload = match object.get("payload") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(map)) => map.clone(),
        Some(_) => {
            return Err(QonversionError::Native {
                message: "remote config payload must be a JSON object".into(),
            });
        }
    };

    let source = match object.get("source") {
        None | Some(Value::Null) => None,
        Some(value) => Some(parse_source(value)?),
    };
    let experiment = match object.get("experiment") {
        None | Some(Value::Null) => None,
        Some(value) => Some(parse_experiment(value)?),
    };

    Ok(RemoteConfig {
        payload,
        source,
        experiment,
    })
}

fn parse_source(value: &Value) -> Result<RemoteConfigurationSource, QonversionError> {
    let object = value.as_object().ok_or_else(|| QonversionError::Native {
        message: "remote config source must be a JSON object".into(),
    })?;
    Ok(RemoteConfigurationSource {
        id: required_string(object, "id")?,
        name: required_string(object, "name")?,
        assignment_type: match object.get("assignment_type").and_then(Value::as_str) {
            Some("auto") => RemoteConfigurationAssignmentType::Auto,
            Some("manual") => RemoteConfigurationAssignmentType::Manual,
            _ => RemoteConfigurationAssignmentType::Unknown,
        },
        source_type: match object.get("type").and_then(Value::as_str) {
            Some("remote_configuration") => RemoteConfigurationSourceType::RemoteConfiguration,
            Some("experiment_control_group") => {
                RemoteConfigurationSourceType::ExperimentControlGroup
            }
            Some("experiment_treatment_group") => {
                RemoteConfigurationSourceType::ExperimentTreatmentGroup
            }
            _ => RemoteConfigurationSourceType::Unknown,
        },
        context_key: object
            .get("context_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(str::to_string),
    })
}

fn parse_experiment(value: &Value) -> Result<Experiment, QonversionError> {
    let object = value.as_object().ok_or_else(|| QonversionError::Native {
        message: "remote config experiment must be a JSON object".into(),
    })?;
    let group_value = object.get("group").ok_or_else(|| QonversionError::Native {
        message: "remote config experiment missing group".into(),
    })?;
    Ok(Experiment {
        id: required_string(object, "id")?,
        name: required_string(object, "name")?,
        group: parse_experiment_group(group_value)?,
    })
}

fn parse_experiment_group(value: &Value) -> Result<ExperimentGroup, QonversionError> {
    let object = value.as_object().ok_or_else(|| QonversionError::Native {
        message: "remote config experiment group must be a JSON object".into(),
    })?;
    Ok(ExperimentGroup {
        id: required_string(object, "id")?,
        name: required_string(object, "name")?,
        group_type: match object.get("type").and_then(Value::as_str) {
            Some("control") => ExperimentGroupType::Control,
            Some("treatment") => ExperimentGroupType::Treatment,
            _ => ExperimentGroupType::Unknown,
        },
    })
}

fn required_string(object: &Map<String, Value>, key: &str) -> Result<String, QonversionError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| QonversionError::Native {
            message: format!("remote config missing string field {key}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_payload_is_success() {
        let config = parse_envelope(r#"{"ok":true,"payload":{}}"#).expect("empty payload");
        assert!(config.payload.is_empty());
        assert!(config.source.is_none());
        assert!(config.experiment.is_none());
    }

    #[test]
    fn missing_payload_is_empty_map() {
        let config = parse_envelope(r#"{"ok":true}"#).expect("missing payload");
        assert!(config.payload.is_empty());
    }

    #[test]
    fn ok_false_is_native_error() {
        let err = parse_envelope(r#"{"ok":false,"error":"network down"}"#)
            .expect_err("ok:false must fail");
        assert_eq!(
            err,
            QonversionError::Native {
                message: "network down".into()
            }
        );
    }

    #[test]
    fn payload_array_is_error() {
        let err = parse_envelope(r#"{"ok":true,"payload":[]}"#).expect_err("array payload");
        assert!(matches!(err, QonversionError::Native { .. }));
    }

    #[test]
    fn invalid_json_is_error() {
        let err = parse_envelope("not-json").expect_err("invalid json");
        assert!(matches!(err, QonversionError::Native { .. }));
    }

    #[test]
    fn source_and_experiment_round_trip() {
        let json = json!({
            "ok": true,
            "payload": { "flag": true, "title": "hello" },
            "source": {
                "id": "cfg-1",
                "name": "Main",
                "assignment_type": "auto",
                "type": "experiment_treatment_group",
                "context_key": "paywall"
            },
            "experiment": {
                "id": "exp-1",
                "name": "Pricing",
                "group": { "id": "g-1", "name": "B", "type": "treatment" }
            }
        });
        let config = parse_envelope(&json.to_string()).expect("full envelope");
        assert_eq!(
            config.payload.get("title").and_then(Value::as_str),
            Some("hello")
        );
        assert_eq!(
            config.payload.get("flag").and_then(Value::as_bool),
            Some(true)
        );
        let source = config.source.expect("source");
        assert_eq!(source.id, "cfg-1");
        assert_eq!(
            source.assignment_type,
            RemoteConfigurationAssignmentType::Auto
        );
        assert_eq!(
            source.source_type,
            RemoteConfigurationSourceType::ExperimentTreatmentGroup
        );
        assert_eq!(source.context_key.as_deref(), Some("paywall"));
        let experiment = config.experiment.expect("experiment");
        assert_eq!(experiment.name, "Pricing");
        assert_eq!(experiment.group.group_type, ExperimentGroupType::Treatment);
    }

    #[test]
    fn unknown_enum_strings_map_to_unknown() {
        let json = json!({
            "ok": true,
            "payload": {},
            "source": {
                "id": "x",
                "name": "y",
                "assignment_type": "future",
                "type": "future",
                "context_key": ""
            },
            "experiment": {
                "id": "e",
                "name": "n",
                "group": { "id": "g", "name": "g", "type": "future" }
            }
        });
        let config = parse_envelope(&json.to_string()).expect("unknown enums");
        let source = config.source.expect("source");
        assert_eq!(
            source.assignment_type,
            RemoteConfigurationAssignmentType::Unknown
        );
        assert_eq!(source.source_type, RemoteConfigurationSourceType::Unknown);
        assert!(source.context_key.is_none());
        assert_eq!(
            config.experiment.expect("experiment").group.group_type,
            ExperimentGroupType::Unknown
        );
    }
}
