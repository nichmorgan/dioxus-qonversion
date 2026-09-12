//! Typed errors for Qonversion primitives.

use thiserror::Error;

/// Errors returned by dioxus-qonversion primitives.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum QonversionError {
    /// Project key or other required config is missing / invalid.
    #[error("invalid Qonversion config: {0}")]
    InvalidConfig(String),

    /// [`crate::initialize`] was already called successfully.
    #[error("Qonversion is already initialized")]
    AlreadyInitialized,

    /// A primitive was called before [`crate::initialize`].
    #[error("Qonversion has not been initialized")]
    NotInitialized,

    /// This crate only supports iOS and Android.
    #[error("unsupported platform: dioxus-qonversion is mobile-only (iOS/Android)")]
    UnsupportedPlatform,

    /// Native SDK classes or the iOS host shim are missing from the app.
    #[error("native Qonversion host missing: {0}")]
    HostMissing(String),

    /// Exception / error forwarded from the native SDK.
    #[error("native Qonversion error: {message}")]
    Native { message: String },
}
