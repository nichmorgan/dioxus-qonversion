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

    /// A queued SDK call was invoked on the Android / iOS UI thread.
    ///
    /// Queued APIs (`identify`, `logout`, `remote_config`, `load_screen`) post
    /// to the main thread and wait. Calling them from that thread deadlocks.
    /// Use Dioxus `spawn` / a background thread.
    #[error("Qonversion SDK call must not run on the UI thread")]
    MainThread,

    /// A queued SDK call exceeded [`crate::sdk_timeout`].
    ///
    /// Timing out does **not** cancel native work. The serial worker is freed
    /// after the native wait expires, so a later queued call may overlap
    /// still-running SDK work. Fail-open vs fail-closed is app policy.
    #[error("Qonversion SDK call timed out after {timeout:?}")]
    Timeout { timeout: Duration },

    /// Native store billing is not connected.
    ///
    /// Typical causes: no Google account signed into Play services, Play Store
    /// missing, or BillingClient `SERVICE_DISCONNECTED` / `BILLING_UNAVAILABLE`.
    /// Distinct from [`Self::Native`] — apps should match this variant and own
    /// fallback UI (for example opening the store).
    #[error("native store billing is unavailable")]
    StoreUnavailable,

    /// No-Codes has no published screen for the given context key.
    ///
    /// Distinct from [`Self::Native`] (transient load failure) and
    /// [`Self::Timeout`]. Show app-owned fallback UI; retrying the same key
    /// will not help until the dashboard publishes a screen.
    #[error("No-Codes screen not found for context key")]
    ScreenNotFound,

    /// Exception / error forwarded from the native SDK.
    #[error("native Qonversion error: {message}")]
    Native { message: String },
}
