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
| **Library** | Qonversion primitives (init, identify, logout, paywall, …), serial SDK queue + default timeout |
| **App** | Project key, stable user id, Remote Config field names, gating UI, fail-open vs fail-closed policy |

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

Both go through one **serial SDK queue** with a default **8s** timeout (`DEFAULT_SDK_TIMEOUT`, override with `set_sdk_timeout`). Timing out returns `QonversionError::Timeout` and does **not** cancel native work — the worker still finishes before the next queued call. `show_screen` stays fire-and-present and is **not** on this queue.

If identify fails or times out, anonymous paywalls can still work. **Fail-open vs fail-closed is app policy** — this crate does not decide. The library also does not memoize Remote Config; clear any app-side caches yourself after `logout`.

## Present a No-Codes screen

After init, present any published screen by its **context key** (from the Qonversion dashboard). The key is always an app parameter — this crate never hardcodes screen names.

```rust
use dioxus_qonversion::show_screen;

show_screen("your_context_key")?;
```

This is **fire-and-present**: it returns once the native SDK has been asked to show the screen, not when the user dismisses it. Finished / failed-to-load callbacks come in a later milestone.

### Android (Dioxus CLI 0.7+)

1. Depend on this crate (`cargo add dioxus-qonversion` / git path).
2. Build with **Dioxus CLI 0.7+** (`dx`). The crate ships a Gradle library under [`android/`](android/) and emits manganis Android artifact metadata so `dx` embeds the Kotlin host automatically — **no copy/paste of Kotlin**.
3. The plugin module already depends on `io.qonversion:no-codes`. You may still list it in `Dioxus.toml` `gradle_dependencies` if you want an explicit app-level pin; it is not required for the host class itself.

Context is taken from `ndk_context` (initialized by Dioxus / wry).

**Fallback (non-`dx` / older CLI):** compile [`android/src/main/kotlin/io/dioxus/qonversion/DioxusQonversionHost.kt`](android/src/main/kotlin/io/dioxus/qonversion/DioxusQonversionHost.kt) into your Android target and add `implementation("io.qonversion:no-codes:1.+")`. Same idea as the iOS Swift host note below — not the happy path.

### iOS app deps

1. Add the [Qonversion iOS SDK](https://github.com/qonversion/qonversion-ios-sdk) via Swift Package Manager (**≥ 6.13.0** for No-Codes).
2. Compile [`ios/DioxusQonversionHost.swift`](ios/DioxusQonversionHost.swift) from this crate into your Dioxus iOS target (copy or path reference in Xcode / your `dx` mobile project).

## License

Licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT license](LICENSE-MIT)

at your option.
