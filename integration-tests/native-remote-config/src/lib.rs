//! Real SDK integration contract for the Android instrumentation test.
#![cfg_attr(not(feature = "native-sdk-test"), allow(dead_code))]

use dioxus_qonversion::{Environment, InitConfig, LaunchMode, RemoteConfig};
use std::time::{Duration, Instant};

const CONTEXT: &str = "dioxus_integration_test";
const PROJECT_KEY: Option<&str> = option_env!("QONVERSION_PROJECT_KEY");

fn initialize() -> Result<(), String> {
    let key = PROJECT_KEY
        .filter(|key| !key.trim().is_empty())
        .ok_or("initialization: QONVERSION_PROJECT_KEY was not supplied at build time")?;
    dioxus_qonversion::set_sdk_timeout(Duration::from_secs(30));
    dioxus_qonversion::initialize(InitConfig {
        project_key: key.to_owned(),
        environment: Environment::Sandbox,
        launch_mode: LaunchMode::SubscriptionManagement,
    })
    .map_err(|error| format!("initialization: {error}"))
}

fn query() -> Result<(), String> {
    let started = Instant::now();
    let config = dioxus_qonversion::remote_config(CONTEXT)
        .map_err(|error| format!("remote_config: {error}"))?;
    validate(&config)?;
    // Never print the key or arbitrary SDK payload/errors.
    println!(
        "PASS native Remote Config: context={CONTEXT}; strict validation approved; elapsed_ms={}",
        started.elapsed().as_millis()
    );
    Ok(())
}

fn validate(config: &RemoteConfig) -> Result<(), String> {
    let payload = &config.payload;
    if payload.is_empty() {
        return Err(
            "payload: empty JSON object (library accepts it; this integration contract does not)"
                .into(),
        );
    }
    let enabled = payload
        .get("feature_test_enabled")
        .ok_or("payload: missing feature_test_enabled")?;
    if enabled.as_bool() != Some(true) {
        return Err("payload: feature_test_enabled must be boolean true".into());
    }
    for (field, expected) in [
        ("test_id", "dioxus_ios_remote_config"),
        ("message", "Remote Config funcionando"),
    ] {
        let value = payload
            .get(field)
            .ok_or_else(|| format!("payload: missing {field}"))?;
        if value.as_str() != Some(expected) {
            return Err(format!(
                "payload: {field} must be a string with the expected value"
            ));
        }
    }
    if let Some(context) = config
        .source
        .as_ref()
        .and_then(|source| source.context_key.as_deref())
    {
        if context != CONTEXT {
            return Err("source: unexpected Context Key".into());
        }
    }
    Ok(())
}

fn sanitize(message: &str) -> String {
    match PROJECT_KEY.filter(|key| !key.is_empty()) {
        Some(key) => message.replace(key, "[REDACTED]"),
        None => message.to_owned(),
    }
}

/// Panics must never unwind through C/JNI. Discard panic details, which may
/// contain arbitrary native data. The runner enforces a separate wall timeout.
fn boundary(work: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    // This crate owns the isolated test process. Prevent the default panic hook
    // from printing arbitrary native values before catch_unwind sanitizes them.
    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|_| {
            eprintln!("Rust integration runner panicked (details suppressed)");
        }))
    });
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .unwrap_or_else(|_| Err("Rust integration runner panicked".into()))
        .map_err(|message| sanitize(&message))
}

#[cfg(all(feature = "native-sdk-test", target_os = "android"))]
mod android;
