//! Remote Config SDK integration demo (Android/iOS).
//!
//! Supply QONVERSION_PROJECT_KEY in the environment before building with dx.
//! The Project Key is compiled into the app; never supply a Secret Key here.

use std::time::{Duration, Instant};

use dioxus::prelude::*;
use dioxus_qonversion::{
    initialize, remote_config, Environment, InitConfig, LaunchMode, QonversionError, RemoteConfig,
};
use futures_channel::oneshot;
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
    Finished {
        elapsed: Duration,
        payload: Option<String>,
        source: Option<String>,
        validation_errors: Vec<String>,
        error: Option<String>,
    },
}

fn main() {
    launch(app);
}

fn project_key() -> Result<String, String> {
    let key = PROJECT_KEY.map(str::trim).filter(|key| !key.is_empty());
    match key {
        None => Err(
            "Inicialização: QONVERSION_PROJECT_KEY ausente. Defina a Project Key de Jusfine - Rust Tests no ambiente do build e recompile o exemplo."
                .into(),
        ),
        Some(key) if key.starts_with("sk_") => Err(
            "Inicialização: a configuração deve conter uma Project Key, não uma Secret Key.".into(),
        ),
        Some(key) => Ok(key.to_owned()),
    }
}

/// Keep a native diagnostic from accidentally echoing the configured key.
fn redact_key(message: String) -> String {
    match PROJECT_KEY.map(str::trim).filter(|key| !key.is_empty()) {
        Some(key) => message.replace(key, "[Project Key ocultada]"),
        None => message,
    }
}

fn sdk_error(error: QonversionError) -> String {
    let message = match error {
        QonversionError::HostMissing(detail) => format!("Host nativo ausente: {detail}"),
        QonversionError::Timeout { timeout } => format!(
            "Timeout: a consulta não terminou em {} ms. O trabalho nativo pode continuar após o timeout.",
            timeout.as_millis()
        ),
        QonversionError::MainThread =>
            "Thread incorreta: a operação bloqueante foi chamada na thread principal.".into(),
        QonversionError::NotInitialized =>
            "Qonversion não inicializada. A consulta exige inicialização bem-sucedida.".into(),
        QonversionError::UnsupportedPlatform =>
            "Plataforma não suportada: o SDK desta biblioteca só funciona no Android e iOS. O desktop apenas permite verificar a compilação da interface.".into(),
        other => other.to_string(),
    };
    redact_key(message)
}

/// Validate each required value without coercing strings/bools or applying defaults.
fn validate_payload(payload: &Value) -> Vec<String> {
    let expected = json!({
        "feature_test_enabled": true,
        "test_id": "dioxus_ios_remote_config",
        "message": "Remote Config funcionando",
    });
    let mut errors = Vec::new();
    for field in ["feature_test_enabled", "test_id", "message"] {
        let expected_value = &expected[field];
        let expected_type = if expected_value.is_boolean() {
            "boolean"
        } else {
            "string"
        };
        match payload.get(field) {
            None => errors.push(format!(
                "{field}: campo ausente (esperado {expected_type})."
            )),
            Some(actual) if actual != expected_value => errors.push(format!(
                "{field}: esperado {expected_value} ({expected_type}); recebido {actual}."
            )),
            Some(_) => {}
        }
    }
    errors
}

fn fetch_remote_config() -> QueryState {
    let started = Instant::now();
    let result = remote_config(CONTEXT_KEY);
    let elapsed = started.elapsed();
    match result {
        Ok(RemoteConfig {
            payload, source, ..
        }) => {
            let payload = Value::Object(payload);
            let validation_errors = validate_payload(&payload);
            match serde_json::to_string_pretty(&payload) {
                Ok(payload) => QueryState::Finished {
                    elapsed,
                    payload: Some(redact_key(payload)),
                    source: source.map(|source| {
                        redact_key(format!(
                            "{} | {} | contexto: {} | {:?}",
                            source.id,
                            source.name,
                            source.context_key.as_deref().unwrap_or("(vazio)"),
                            source.source_type,
                        ))
                    }),
                    validation_errors: validation_errors.into_iter().map(redact_key).collect(),
                    error: None,
                },
                Err(error) => QueryState::Finished {
                    elapsed,
                    payload: None,
                    source: None,
                    validation_errors: Vec::new(),
                    error: Some(format!(
                        "Payload inválido: não foi possível exibir o JSON: {error}"
                    )),
                },
            }
        }
        Err(error) => QueryState::Finished {
            elapsed,
            payload: None,
            source: None,
            validation_errors: Vec::new(),
            error: Some(sdk_error(error)),
        },
    }
}

/// Same worker -> async channel -> UI pattern as dioxus-firebase's spawn_auth.
/// Awaiting the receiver never blocks the main loop needed by the native SDK.
fn spawn_query(mut state: Signal<QueryState>) {
    state.set(QueryState::Loading);
    spawn(async move {
        let started = Instant::now();
        let (tx, rx) = oneshot::channel();
        let worker = std::thread::Builder::new()
            .name("qonversion-demo-query".into())
            .spawn(move || {
                let _ = tx.send(fetch_remote_config());
            });
        let result = match worker {
            Ok(_) => rx.await.map_err(|_| {
                "A thread de consulta terminou sem devolver um resultado.".to_string()
            }),
            Err(error) => Err(format!(
                "Não foi possível iniciar a thread de consulta: {error}"
            )),
        };
        state.set(result.unwrap_or_else(|error| QueryState::Finished {
            elapsed: started.elapsed(),
            payload: None,
            source: None,
            validation_errors: Vec::new(),
            error: Some(error),
        }));
    });
}

fn app() -> Element {
    let mut initialization = use_signal(|| Initialization::Pending);
    let query = use_signal(|| QueryState::Idle);

    // Initialize after the native app/UI runtime exists, as in the Firebase demo.
    // Do not read reactive state in this effect: initialization must run only once.
    use_effect(move || {
        let result = project_key().and_then(|project_key| {
            initialize(InitConfig {
                project_key,
                environment: Environment::Sandbox,
                launch_mode: LaunchMode::SubscriptionManagement,
            })
            .map_err(|error| format!("Falha de inicialização: {}", sdk_error(error)))
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
            p { "Projeto esperado: Jusfine - Rust Tests" }
            p { "Context Key: {CONTEXT_KEY} · Ambiente: Sandbox" }
            div {
                id: "initialization-status",
                role: "status",
                match &*initialization.read() {
                    Initialization::Pending => rsx! { p { "Inicializando Qonversion..." } },
                    Initialization::Ready => rsx! { p { "SDK inicializado. Pronto para consultar." } },
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
                "Consultar Remote Config"
            }
            section {
                id: "remote-config-result",
                "aria-live": "polite",
                match &*query.read() {
                    QueryState::Idle => rsx! { p { "Nenhuma consulta executada." } },
                    QueryState::Loading => rsx! { p { "Consultando Remote Config..." } },
                    QueryState::Finished { elapsed, payload, source, validation_errors, error } => rsx! {
                        p { "Tempo de execução: {elapsed.as_millis()} ms" }
                        if let Some(error) = error {
                            p { role: "alert", style: "color: #b91c1c;", "Falha na consulta: {error}" }
                        }
                        if let Some(payload) = payload {
                            h2 { "JSON recebido" }
                            pre { style: "white-space: pre-wrap; background: #f1f5f9; padding: 16px;", "{payload}" }
                            if validation_errors.is_empty() {
                                p { id: "payload-validation", "Payload válido: os três campos têm os tipos e valores esperados." }
                            } else {
                                div { role: "alert", style: "color: #b91c1c;",
                                    p { "Payload inválido:" }
                                    ul {
                                        for error in validation_errors {
                                            li { "{error}" }
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(source) = source {
                            p { "Origem informada pelo SDK: {source}" }
                        }
                    },
                }
            }
            p { "Consultas repetidas podem usar o cache do SDK. Para testar uma nova sessão, encerre e abra o aplicativo." }
        }
    }
}
