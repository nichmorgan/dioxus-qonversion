//! Real Remote Config check against a Qonversion test project.
//!
//! Set `QONVERSION_PROJECT_KEY` before building with `dx`. The key is compiled
//! into the app. Supply the SDK Project Key, not a Secret Key.

use std::time::{Duration, Instant};

use dioxus::prelude::*;
use dioxus_qonversion::{
    initialize, remote_config_async, set_sdk_timeout, Environment, InitConfig, LaunchMode,
    QonversionError, RemoteConfig,
};
use serde_json::{json, Value};

const PROJECT_KEY: Option<&str> = option_env!("QONVERSION_PROJECT_KEY");
const CONTEXT_KEY: &str = "dioxus_integration_test";

#[derive(Clone, PartialEq)]
enum Initialization {
    Pending,
    Ready,
    Failed(String),
}

#[derive(Clone, PartialEq)]
enum QueryState {
    Idle,
    Loading,
    Failed {
        elapsed: Duration,
        message: String,
    },
    Ready {
        elapsed: Duration,
        payload: String,
        source: Option<String>,
        validation_errors: Vec<String>,
    },
}

fn main() {
    dioxus::launch(app);
}

fn project_key() -> Result<String, String> {
    match PROJECT_KEY.map(str::trim).filter(|key| !key.is_empty()) {
        None => Err(
            "Initialization: QONVERSION_PROJECT_KEY is missing. Set the SDK Project Key and rebuild."
                .into(),
        ),
        Some(key) if key.starts_with("sk_") => {
            Err("Initialization: set an SDK Project Key, not a Secret Key.".into())
        }
        Some(key) => Ok(key.to_owned()),
    }
}

fn redact_key(message: String) -> String {
    match PROJECT_KEY.map(str::trim).filter(|key| !key.is_empty()) {
        Some(key) => message.replace(key, "[Project Key hidden]"),
        None => message,
    }
}

fn sdk_error(error: QonversionError) -> String {
    let message = match error {
        QonversionError::HostMissing(detail) => format!("Native host missing: {detail}"),
        QonversionError::Timeout { timeout } => format!(
            "Timeout: the query did not finish in {} ms. Native work may continue after the timeout.",
            timeout.as_millis()
        ),
        QonversionError::MainThread => {
            "Wrong thread: a blocking SDK call ran on the main thread.".into()
        }
        QonversionError::NotInitialized => {
            "Qonversion is not initialized. The query needs a successful initialization.".into()
        }
        QonversionError::UnsupportedPlatform => {
            "Unsupported platform: this SDK runs on Android and iOS only.".into()
        }
        other => other.to_string(),
    };
    redact_key(message)
}

fn validate_payload(payload: &Value) -> Vec<String> {
    let expected = [
        ("feature_test_enabled", json!(true)),
        ("test_id", json!("dioxus_ios_remote_config")),
        ("message", json!("Remote Config funcionando")),
    ];
    expected
        .into_iter()
        .filter_map(|(field, expected_value)| {
            let expected_type = if expected_value.is_boolean() {
                "boolean"
            } else {
                "string"
            };
            match payload.get(field) {
                None => Some(format!("{field}: missing (expected {expected_type}).")),
                Some(actual) if actual != &expected_value => Some(format!(
                    "{field}: expected {expected_value} ({expected_type}); received {actual}."
                )),
                Some(_) => None,
            }
        })
        .collect()
}

fn finished(elapsed: Duration, result: Result<RemoteConfig, QonversionError>) -> QueryState {
    match result {
        Ok(RemoteConfig { payload, source, .. }) => {
            let payload = Value::Object(payload);
            let validation_errors = validate_payload(&payload)
                .into_iter()
                .map(redact_key)
                .collect();
            let payload = serde_json::to_string_pretty(&payload)
                .unwrap_or_else(|error| format!("could not display JSON: {error}"));
            QueryState::Ready {
                elapsed,
                payload: redact_key(payload),
                source: source.map(|source| {
                    redact_key(format!(
                        "{} | {} | context: {} | {:?}",
                        source.id,
                        source.name,
                        source.context_key.as_deref().unwrap_or("(empty)"),
                        source.source_type,
                    ))
                }),
                validation_errors,
            }
        }
        Err(error) => QueryState::Failed {
            elapsed,
            message: sdk_error(error),
        },
    }
}

fn spawn_query(mut state: Signal<QueryState>) {
    state.set(QueryState::Loading);
    let started = Instant::now();
    spawn(async move {
        let result = remote_config_async(CONTEXT_KEY).await;
        state.set(finished(started.elapsed(), result));
    });
}

fn app() -> Element {
    let mut initialization = use_signal(|| Initialization::Pending);
    let query = use_signal(|| QueryState::Idle);

    use_effect(move || {
        set_sdk_timeout(Duration::from_secs(20));
        let result = project_key().and_then(|project_key| {
            initialize(InitConfig {
                project_key,
                environment: Environment::Sandbox,
                launch_mode: LaunchMode::SubscriptionManagement,
            })
            .map_err(|error| format!("Initialization failed: {}", sdk_error(error)))
        });
        initialization.set(match result {
            Ok(()) => Initialization::Ready,
            Err(error) => Initialization::Failed(error),
        });
    });

    let ready = matches!(&*initialization.read(), Initialization::Ready);
    let loading = matches!(&*query.read(), QueryState::Loading);

    rsx! {
        main {
            style: "padding: 24px; max-width: 680px; margin: 0 auto; font-family: sans-serif; overflow-wrap: anywhere;",
            h1 { "Qonversion Remote Config" }
            p { "Context key: {CONTEXT_KEY} · Environment: Sandbox" }
            div {
                id: "initialization-status",
                role: "status",
                match &*initialization.read() {
                    Initialization::Pending => rsx! { p { "Initializing Qonversion..." } },
                    Initialization::Ready => rsx! { p { "SDK initialized. Ready to query." } },
                    Initialization::Failed(error) => rsx! {
                        p { role: "alert", style: "color: #b91c1c;", "{error}" }
                    },
                }
            }
            button {
                id: "fetch-remote-config",
                disabled: !ready || loading,
                style: "padding: 12px 16px; margin: 12px 0;",
                onclick: move |_| {
                    if ready && !loading {
                        spawn_query(query);
                    }
                },
                "Fetch Remote Config"
            }
            section {
                id: "remote-config-result",
                "aria-live": "polite",
                match &*query.read() {
                    QueryState::Idle => rsx! { p { "No query yet." } },
                    QueryState::Loading => rsx! { p { "Fetching Remote Config..." } },
                    QueryState::Failed { elapsed, message } => rsx! {
                        p { "Elapsed: {elapsed.as_millis()} ms" }
                        p { role: "alert", style: "color: #b91c1c;", "Query failed: {message}" }
                    },
                    QueryState::Ready { elapsed, payload, source, validation_errors } => rsx! {
                        p { "Elapsed: {elapsed.as_millis()} ms" }
                        h2 { "Received JSON" }
                        pre { style: "white-space: pre-wrap; background: #f1f5f9; padding: 16px;", "{payload}" }
                        if validation_errors.is_empty() {
                            p { id: "payload-validation", "Payload valid: the three fields have the expected types and values." }
                        } else {
                            div { role: "alert", style: "color: #b91c1c;",
                                p { "Payload invalid:" }
                                ul {
                                    for error in validation_errors {
                                        li { "{error}" }
                                    }
                                }
                            }
                        }
                        if let Some(source) = source {
                            p { "SDK source: {source}" }
                        }
                    },
                }
            }
            p { "Repeated queries can use the SDK cache. Restart the app to start a new session." }
        }
    }
}
