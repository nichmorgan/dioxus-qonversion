//! Typed errors for Qonversion primitives.

use std::time::Duration;

use thiserror::Error;

/// Errors returned by dioxus-qonversion primitives.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum QonversionError {
    /// A required string is empty or invalid (project key, context key, or user id).
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

    /// A queued SDK call exceeded [`crate::sdk_timeout`].
    ///
    /// Timing out does **not** cancel native work — it only unblocks the Rust
    /// caller. Fail-open vs fail-closed is app policy.
    #[error("Qonversion SDK call timed out after {timeout:?}")]
    Timeout { timeout: Duration },

    /// Exception / error forwarded from the native SDK.
    #[error("native Qonversion error: {message}")]
    Native { message: String },
}
