[![Discord Server](https://img.shields.io/discord/899851952891002890.svg?logo=discord&style=flat-square)](https://discord.gg/sKJSVNSCDJ)

# dioxus-qonversion

Unofficial [Dioxus](https://dioxuslabs.com/) bridge for [Qonversion](https://qonversion.io) Subscription Management Mode.

**Mobile only for now** (iOS and Android). Web and desktop are out of scope. Qonversion’s own [Web SDK](https://documentation.qonversion.io/docs/web-sdk) covers browser checkout (Stripe/Paddle); this crate does not wrap it.

This crate is **not affiliated** with Qonversion.

## Status

Early — the public API is not stable yet. See the [roadmap](https://github.com/nichmorgan/dioxus-qonversion/issues/1).

## Architecture

```text
Dioxus UI (Rust)
  → dioxus-qonversion (Rust API)
    → Kotlin host (Android) / Swift host (iOS)
      → Qonversion SDK + No-Codes SDK
        → App Store / Play Store
```

Native calls go through a thin host — Kotlin on Android (auto-bundled by Dioxus CLI 0.7+), ObjC-visible Swift on iOS — not UniFFI.

## Ownership

| Layer | Owns |
| -- | -- |
| **Library** | Qonversion primitives (init, identify, logout, paywall, entitlements, restore, …), serial SDK queue + default timeout |
| **App** | Project key, stable user id, Remote Config field names, which entitlement id is premium, gating UI, fail-open vs fail-closed policy |

## Initialize

Call once before any other library API. The project key comes from the app (never hardcode it in a library).

```rust
use dioxus_qonversion::{initialize, Environment, InitConfig, LaunchMode};

initialize(InitConfig {
    project_key: std::env::var("QONVERSION_PROJECT_KEY").expect("set QONVERSION_PROJECT_KEY"),
    environment: Environment::Sandbox, // Production for store releases
    launch_mode: LaunchMode::SubscriptionManagement,
})?;
```

Double-init returns a typed error. Desktop / web builds return `UnsupportedPlatform`.

## Identify / logout

After init (and after your app session restore / sign-in), map a **stable** app user id into Qonversion so purchases attach to that user. Use your auth provider uid — not an ephemeral session token.

```rust
use dioxus_qonversion::{identify, logout};

// Prefer calling from Dioxus `spawn` / a background thread (blocking wait).
identify(firebase_uid)?;

// On sign-out:
logout()?;
```

`identify` / `logout`, Remote Config, `load_screen`, `check_entitlements`, and `restore` go through one **serial SDK queue** with a default **8s** timeout (`DEFAULT_SDK_TIMEOUT`, override with `set_sdk_timeout`). Timing out returns `QonversionError::Timeout` and does **not** cancel native work — the worker is freed after the wait expires, so a later queued call may overlap still-running SDK work. Calling these from the UI thread returns `QonversionError::MainThread`. `show_screen` stays fire-and-present and is **not** on this queue.

If identify fails or times out, anonymous paywalls can still work. **Fail-open vs fail-closed is app policy** — this crate does not decide. The library does not memoize Remote Config or entitlements; clear any app-side caches yourself after `identify` / `logout`. The native SDK also drops its **in-memory** Remote Config cache on those calls (identify cache-drop even with the same id needs iOS 6.14.0+ / Android 9.7.0+), so refetch `remote_config` afterwards if you still need the payload.

## Remote Config

After init, fetch the JSON payload for any dashboard **context key**. Field names inside the payload stay app-owned — this crate does not interpret them.

```rust
use dioxus_qonversion::{remote_config, remote_config_default};

// Prefer calling from Dioxus `spawn` / a background thread (blocking wait).
let config = remote_config("your_context_key")?;
let payload = &config.payload;

// Empty dashboard context key (SDK default overload):
let default = remote_config_default()?;
```

Same **serial SDK queue** and **8s** timeout as `identify` / `logout`. An empty payload map is success; SDK failures are `QonversionError::Native` or `Timeout`. `source` and `experiment` are present when the SDK assigned this payload from a remote config or A/B experiment.

### Feature flags (app-owned)

Field names stay caller-provided. Treat missing / wrong-type bools as `false` (fail-closed) in the **app**:

```rust
let enabled = config
    .payload
    .get("your_flag_field")
    .and_then(|value| value.as_bool())
    .unwrap_or(false);
```

## Load a No-Codes screen (optional)

`load_screen` is Qonversion’s **ask-first** `loadScreen`: it waits until the screen is in cache (or fails) **before** anything is presented. A success warms the shared screens cache so the next `show_screen` with the same key can render without the SDK loading view. It is **not** a prerequisite for `show_screen`. Official `loadScreen` does not flush pending user properties, so targeting can differ slightly from `show_screen`.

Screens marked **preloadable** in the No-Codes Builder (Settings → General) are fetched automatically at SDK init — that path needs no crate API.

```rust
use dioxus_qonversion::{load_screen, show_screen, QonversionError};

// Prefer calling from Dioxus `spawn` / a background thread (blocking wait).
match load_screen("your_context_key") {
    Ok(_screen) => show_screen("your_context_key")?,
    Err(QonversionError::ScreenNotFound) => { /* app-owned fallback — key has no published screen */ }
    Err(QonversionError::Timeout { .. }) | Err(QonversionError::Native { .. }) => {
        /* transient; retry or fallback */
    }
    other => other.map(|_| ())?,
}
```

A successful load also returns official Builder defaults: `default_selected_product_id` and `default_variables` (custom variables and product slots). Same **serial SDK queue** and **8s** timeout as `identify` / `logout` / Remote Config.

## Check entitlements / restore

After init, read the current entitlement map. An empty map is success — the user has no entitlements. The app owns which dashboard id means “premium”.

```rust
use dioxus_qonversion::{check_entitlements, restore};

// Prefer calling from Dioxus `spawn` / a background thread (blocking wait).
let entitlements = check_entitlements()?;
let premium = entitlements
    .get("your_entitlement_id")
    .map(|e| e.is_active)
    .unwrap_or(false);

// After the user taps Restore (or after ActionFinished { Restore } from a No-Codes screen):
let entitlements = restore()?;
```

`restore` maps to official `restore()`: it refreshes Store-linked entitlements and returns the same map shape. It does not create a purchase or charge the user. Re-run `check_entitlements` later if you need a fresh snapshot. Same **serial SDK queue** and **8s** timeout as `identify`.

### check_status recipe

Compose Remote Config + entitlements in the **app**. Example field name `entitlement_id` is an example only — never a crate constant:

```rust
use dioxus_qonversion::{check_entitlements, remote_config};

let config = remote_config("your_context_key")?;
let entitlement_id = config
    .payload
    .get("entitlement_id")
    .and_then(|value| value.as_str())
    .unwrap_or("premium");
let entitlements = check_entitlements()?;
let active = entitlements
    .get(entitlement_id)
    .map(|e| e.is_active)
    .unwrap_or(false);
```

Call this after launch, after `identify`, and after `ActionFinished { Purchase }` / `Restore`.

## Present a No-Codes screen

After init, present any published screen by its **context key** (from the Qonversion dashboard). The key is always an app parameter — this crate never hardcodes screen names.

```rust
use dioxus_qonversion::show_screen;

show_screen("your_context_key")?;
```

This is **fire-and-present**: it returns once the native SDK has been asked to show the screen, not when the user dismisses it.

On **Android**, `show_screen` may return `QonversionError::StoreUnavailable` **before** present when Play Billing is not connected (unsigned Play account, missing Play Store, `BILLING_UNAVAILABLE` / `SERVICE_DISCONNECTED`). Call it from Dioxus `spawn` / a background thread so that preflight can wait without deadlocking the main looper. UI-thread calls skip the blocking probe (happy path unchanged).

If product fetch still fails **after** present, the process-wide handler from `set_screen_failed_handler` fires with `StoreUnavailable` (Play billing) or `Native` (other load errors). The app owns fallback UI — this crate does not show a dialog or open the Play Store.

Purchases are **not** `Finished`. Register `set_screen_event_handler` and match `ActionFinished { Purchase }` (or `Restore`). `Finished` means the flow closed — dismiss or Close All — and is not a buy.

```rust
use dioxus_qonversion::{
    set_screen_event_handler, set_screen_failed_handler, show_screen, QonversionError,
    ScreenActionKind, ScreenEvent,
};

set_screen_failed_handler(|err| match err {
    QonversionError::StoreUnavailable => { /* app-owned fallback UI */ }
    QonversionError::Native { message } => { /* log / other fallback */ }
});

set_screen_event_handler(|event| match event {
    ScreenEvent::ActionFinished { kind: ScreenActionKind::Purchase } => {
        /* buy succeeded — refresh entitlements / status; queued APIs are safe here */
    }
    ScreenEvent::ActionFinished { kind: ScreenActionKind::Restore } => {
        /* restore-from-paywall succeeded */
    }
    ScreenEvent::ActionFailed { kind: ScreenActionKind::Purchase, message } => {
        /* buy failed; screen may stay open */
        let _ = message;
    }
    ScreenEvent::Finished => { /* flow closed; do not treat as purchased */ }
    ScreenEvent::CustomAction { value } => { /* builder custom action; screen stays open */ let _ = value; }
    _ => {}
});

// Prefer `spawn` on Android so preflight can return StoreUnavailable without flashing UI.
match show_screen("your_context_key") {
    Err(QonversionError::StoreUnavailable) => { /* app-owned fallback UI */ }
    other => other?,
}
```

### Android (Dioxus CLI 0.7+)

1. Depend on this crate (`cargo add dioxus-qonversion` / git path).
2. Build with **Dioxus CLI 0.7+** (`dx`). The crate ships a Gradle library under [`android/`](android/) and emits manganis Android artifact metadata so `dx` embeds the Kotlin host automatically — **no copy/paste of Kotlin**.
3. The plugin module already depends on `io.qonversion:no-codes`. You may still list it in `Dioxus.toml` `gradle_dependencies` if you want an explicit app-level pin; it is not required for the host class itself.

Context is taken from `ndk_context` (initialized by Dioxus / wry).

**Fallback (non-`dx` / older CLI):** compile [`android/src/main/kotlin/io/dioxus/qonversion/DioxusQonversionHost.kt`](android/src/main/kotlin/io/dioxus/qonversion/DioxusQonversionHost.kt) into your Android target and add `implementation("io.qonversion:no-codes:1.+")`. Same idea as the iOS Swift host note below — not the happy path.

### iOS app deps

1. Add the [Qonversion iOS SDK](https://github.com/qonversion/qonversion-ios-sdk) via Swift Package Manager (**≥ 6.13.0** for No-Codes).
2. Compile [`ios/DioxusQonversionHost.swift`](ios/DioxusQonversionHost.swift) from this crate into your Dioxus iOS target (copy or path reference in Xcode / your `dx` mobile project).

## App recipes

These are **not** library features — they show how to compose primitives. Example keys like `"paywall"` are examples only, never crate constants. See [#5](https://github.com/nichmorgan/dioxus-qonversion/issues/5), [#4](https://github.com/nichmorgan/dioxus-qonversion/issues/4), [#6](https://github.com/nichmorgan/dioxus-qonversion/issues/6), [#9](https://github.com/nichmorgan/dioxus-qonversion/issues/9), [#10](https://github.com/nichmorgan/dioxus-qonversion/issues/10).

1. **identify on session** — after sign-in / restore, `identify(stable_user_id)` (best-effort; paywalls still work if it fails). Then refetch `remote_config` if you cached a previous payload.
2. **present_paywall** — `show_screen(context_key)` with a key from app config or Remote Config. Refresh status on `ActionFinished { Purchase }` (not on `Finished`).
3. **check_status** — `remote_config(key)` → read the app-defined entitlement id field → `check_entitlements()` → `is_active`.
4. **feature flags** — `remote_config(key)` → fail-closed bools in the **app** (missing / wrong type → `false`).

## License

Licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT license](LICENSE-MIT)

at your option.
