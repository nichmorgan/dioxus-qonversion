//! Register the Android Gradle host module with Dioxus CLI via manganis.
//!
//! This emits `AndroidArtifactMetadata` so `dx` 0.7+ embeds
//! [`android/`](../../../android/) as `implementation(project(":plugins:…"))`.
//! Runtime calls stay in [`super::android`] (hand JNI) — the generated type is unused.

#![allow(dead_code)]

#[manganis::ffi("android")]
extern "Kotlin" {
    /// Present only so manganis emits plugin metadata for `dx`.
    type DioxusQonversionHost;
}
