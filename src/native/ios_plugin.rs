//! Register the existing iOS Swift host package with Dioxus CLI via manganis.
//!
//! Emits SwiftPackageMetadata so dx 0.7+ compiles ios/ (product
//! DioxusQonversionHost) and embeds the framework. [`DioxusQonversionHost::new`]
//! also loads that framework. Method calls stay on the hand-written bridge in
//! [`super::ios`].

#[manganis::ffi("ios")]
extern "Swift" {
    /// Constructing this loads `DioxusSwiftPlugins.framework`. Selectors are manual.
    type DioxusQonversionHost;
}
