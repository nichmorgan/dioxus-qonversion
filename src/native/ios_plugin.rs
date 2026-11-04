//! Register the existing iOS Swift host package with Dioxus CLI via manganis.
//!
//! Emits SwiftPackageMetadata so dx 0.7+ compiles ios/ (product
//! DioxusQonversionHost) and embeds the framework. Runtime calls still use
//! super::ios and the existing Objective-C selectors, not generated bindings.

#![allow(dead_code)]

#[manganis::ffi("ios")]
extern "Swift" {
    /// Metadata registration only; runtime calls use the hand-written bridge.
    type DioxusQonversionHost;
}
