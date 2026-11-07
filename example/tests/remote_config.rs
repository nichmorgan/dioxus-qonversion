//! Live Remote Config and No-Codes `load_screen` check. Boot an Android emulator
//! or iOS simulator, install the Dioxus CLI (`cargo binstall dioxus-cli@0.7.10`),
//! then from `example/`: `cargo test -- --ignored --nocapture android` (or `ios`).
//!
//! `remote_config` and `no_code` share one `dx` run per platform.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use rstest::rstest;
use serde_json::Value;

const CONTEXT_KEY: &str = "dioxus_integration_test";

fn live_report(platform: &str, target: &str, device: &[&str]) -> Value {
    static REPORTS: Mutex<Option<HashMap<String, Value>>> = Mutex::new(None);
    let mut reports = REPORTS.lock().expect("live report lock");
    let reports = reports.get_or_insert_with(HashMap::new);
    if let Some(report) = reports.get(platform) {
        return report.clone();
    }
    let report = run_dx(platform, target, device);
    reports.insert(platform.to_owned(), report.clone());
    report
}

fn run_dx(platform: &str, target: &str, device: &[&str]) -> Value {
    let mut app = Command::new("dx")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("QONVERSION_REMOTE_CONFIG_CONTEXT_KEY", CONTEXT_KEY)
        .stdout(Stdio::piped())
        .args(["run", "--platform", platform, "--target", target])
        .args(device)
        .args(["--interactive", "false"])
        .spawn()
        .expect("install the Dioxus CLI: cargo binstall dioxus-cli@0.7.10");
    let mut dx_lines = Vec::new();
    let line = BufReader::new(app.stdout.take().expect("piped stdout"))
        .lines()
        .map_while(Result::ok)
        .find_map(|line| {
            let found = line
                .split_once("QONVERSION_E2E ")
                .map(|(_, report)| report.to_owned());
            if found.is_none() {
                dx_lines.push(line);
            }
            found
        });
    let _ = app.kill();
    let _ = app.wait();

    let line = line.unwrap_or_else(|| {
        let start = dx_lines.len().saturating_sub(20);
        panic!(
            "dx exited before the app reported\n{}",
            dx_lines[start..].join("\n")
        );
    });
    serde_json::from_str(&line).expect("the QONVERSION_E2E line is valid JSON")
}

/// The project key and the No-Codes context key come from the environment or `.env`.
/// This test pins only the Remote Config context key.
#[rstest]
// Without `--device`, dx boots the first AVD even when an emulator is already running.
// `--device` fuzzy-matches the adb serial (`emulator-5554`), not the AVD name.
#[case::android("android", "x86_64-linux-android", &["--device", "emulator"])]
#[case::ios("ios", "aarch64-apple-ios-sim", &[])]
#[ignore = "needs a booted emulator or simulator and the Dioxus CLI"]
fn remote_config(#[case] platform: &str, #[case] target: &str, #[case] device: &[&str]) {
    let report = live_report(platform, target, device);
    assert!(report["error"].is_null(), "{}", report["error"]);
    let payload = report["payload"]
        .as_object()
        .unwrap_or_else(|| panic!("the Remote Config payload is not a JSON object: {report}"));
    assert!(
        !payload.is_empty(),
        "expected a non-empty Remote Config, got {report}"
    );
    println!("remote config keys: {}", payload.len());
}

/// Same `dx` run as [`remote_config`]. Asserts `load_screen` returned a screen id.
#[rstest]
#[case::android("android", "x86_64-linux-android", &["--device", "emulator"])]
#[case::ios("ios", "aarch64-apple-ios-sim", &[])]
#[ignore = "needs a booted emulator or simulator and the Dioxus CLI"]
fn no_code(#[case] platform: &str, #[case] target: &str, #[case] device: &[&str]) {
    let report = live_report(platform, target, device);
    assert!(
        report["screen_error"].is_null(),
        "{}",
        report["screen_error"]
    );
    let screen_id = report["screen_id"].as_str().unwrap_or("");
    assert!(
        !screen_id.is_empty(),
        "expected a No-Codes screen id, got {report}"
    );
    println!("screen id present");
}
