//! Home screen of square feature buttons. Remote Config, No-Code, User Properties.
//!
//! Remote Config and `load_screen` run when the app launches, not when a button
//! is tapped, so the e2e test can read the `QONVERSION_E2E` line without touching
//! the UI. That line carries the payload and a No-Codes screen id. The No-Code
//! button still calls `show_screen`. User Properties is a manual demo only.
//!
//! `cp .env.example .env`, set the SDK Project Key, then `dx serve --platform android`.

use dioxus::prelude::*;
use dioxus_qonversion::{
    initialize, load_screen_async, remote_config_async, remote_config_default_async,
    set_custom_user_property_async, set_user_property_async, show_screen, Environment, InitConfig,
    LaunchMode, QonversionError, RemoteConfig, UserPropertyKey,
};
use serde_json::{json, Value};

const STYLES: &str = r#"
body { font-family: sans-serif; margin: 0; }
h1 { font-size: 1.4rem; margin: 16px; }
.grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 16px;
    padding: 16px;
}
.tile {
    aspect-ratio: 1;
    border: 0;
    border-radius: 16px;
    background: #3d3a8c;
    color: white;
    font-size: 1.05rem;
    font-weight: 600;
}
.back {
    margin: 16px 16px 0;
    padding: 8px 0;
    border: 0;
    background: transparent;
    font-size: 1rem;
}
.action {
    margin: 16px;
    padding: 12px 16px;
    border: 0;
    border-radius: 12px;
    background: #3d3a8c;
    color: white;
    font-size: 1rem;
    font-weight: 600;
}
pre, .status { margin: 16px; white-space: pre-wrap; word-break: break-word; }
"#;

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Home,
    RemoteConfig,
    UserProperties,
}

fn main() {
    dioxus::launch(app);
}

fn app() -> Element {
    let mut screen = use_signal(|| Screen::Home);
    let mut no_code_error = use_signal(|| None::<String>);
    let mut user_properties_status = use_signal(|| None::<String>);
    // Runs at launch. The Remote Config screen only displays this result.
    let report = use_resource(|| async {
        let report = match fetch().await {
            Err(error) => json!({ "error": error.to_string() }),
            Ok(config) => match load_no_code().await {
                Ok(screen_id) => json!({ "payload": config.payload, "screen_id": screen_id }),
                Err(error) => json!({
                    "payload": config.payload,
                    "screen_error": error.to_string(),
                }),
            },
        };
        // The e2e tests read this line from the app's stdout.
        println!("QONVERSION_E2E {report}");
        report
    });
    let current = screen();

    rsx! {
        style { {STYLES} }
        match current {
            Screen::Home => rsx! {
                h1 { "Qonversion" }
                div { class: "grid",
                    button {
                        class: "tile",
                        onclick: move |_| screen.set(Screen::RemoteConfig),
                        "Remote Config"
                    }
                    button {
                        class: "tile",
                        onclick: move |_| {
                            spawn(async move {
                                let key = option_env!("QONVERSION_NO_CODE_CONTEXT_KEY")
                                    .unwrap_or("");
                                match show_screen(key) {
                                    Ok(()) => no_code_error.set(None),
                                    Err(error) => no_code_error.set(Some(error.to_string())),
                                }
                            });
                        },
                        "No-Code"
                    }
                    button {
                        class: "tile",
                        onclick: move |_| {
                            user_properties_status.set(None);
                            screen.set(Screen::UserProperties);
                        },
                        "User Properties"
                    }
                }
                if let Some(error) = no_code_error() {
                    p { class: "status", "Error: {error}" }
                }
            },
            Screen::RemoteConfig => rsx! {
                button {
                    class: "back",
                    onclick: move |_| screen.set(Screen::Home),
                    "Back"
                }
                h1 { "Remote Config" }
                {remote_config_body(report.cloned())}
            },
            Screen::UserProperties => rsx! {
                button {
                    class: "back",
                    onclick: move |_| screen.set(Screen::Home),
                    "Back"
                }
                h1 { "User Properties" }
                button {
                    class: "action",
                    onclick: move |_| {
                        spawn(async move {
                            match set_demo_user_properties().await {
                                Ok(()) => user_properties_status
                                    .set(Some("Set email and liked_app.".into())),
                                Err(error) => {
                                    user_properties_status.set(Some(error.to_string()))
                                }
                            }
                        });
                    },
                    "Set demo properties"
                }
                if let Some(status) = user_properties_status() {
                    p { class: "status", "{status}" }
                }
            },
        }
    }
}

fn remote_config_body(report: Option<Value>) -> Element {
    match report {
        None => rsx! { p { "Loading..." } },
        Some(report) => match report["error"].as_str() {
            Some(error) => rsx! { p { "Error: {error}" } },
            None => {
                let payload = format!("{:#}", report["payload"]);
                rsx! { pre { "{payload}" } }
            }
        },
    }
}

async fn set_demo_user_properties() -> Result<(), QonversionError> {
    // Init runs in the launch `use_resource`; require it before setting properties.
    set_user_property_async(UserPropertyKey::Email, "demo@example.com").await?;
    set_custom_user_property_async("liked_app", "yes").await?;
    Ok(())
}

async fn load_no_code() -> Result<String, QonversionError> {
    let screen =
        load_screen_async(option_env!("QONVERSION_NO_CODE_CONTEXT_KEY").unwrap_or("")).await?;
    Ok(screen.id)
}

async fn fetch() -> Result<RemoteConfig, QonversionError> {
    initialize(InitConfig {
        project_key: option_env!("QONVERSION_PROJECT_KEY")
            .unwrap_or_default()
            .into(),
        environment: Environment::Sandbox,
        launch_mode: LaunchMode::SubscriptionManagement,
    })?;
    match option_env!("QONVERSION_REMOTE_CONFIG_CONTEXT_KEY").filter(|key| !key.is_empty()) {
        Some(context_key) => remote_config_async(context_key).await,
        None => remote_config_default_async().await,
    }
}
