//! User identity primitives (`identify` / `logout`).

use crate::error::QonversionError;
use crate::init;
use crate::native;
use crate::queue;

/// Map a stable app user id into Qonversion so purchases attach to that user.
///
/// Call after session restore / sign-in with your auth provider’s uid (or other
/// stable id). Prefer a **stable** id — do not pass ephemeral session tokens.
///
/// Runs on the serial SDK queue with [`crate::DEFAULT_SDK_TIMEOUT`] (see
/// [`crate::set_sdk_timeout`]). Blocking wait: safe to call from Dioxus `spawn`
/// / a background thread. The UI thread returns [`QonversionError::MainThread`].
pub fn identify(user_id: &str) -> Result<(), QonversionError> {
    let user_id = crate::helpers::require_non_empty("user_id", user_id)?;
    init::require_initialized()?;
    let user_id = user_id.to_string();
    queue::run_serial(move || native::identify(&user_id))
}

/// Clear the Qonversion user session (call on app logout).
///
/// Runs on the same serial SDK queue as [`identify`]. Blocking wait — prefer
/// `spawn` / a background thread.
///
/// This library does not memoize Remote Config or entitlements for the app;
/// clear any app-side caches yourself after logout.
pub fn logout() -> Result<(), QonversionError> {
    init::require_initialized()?;
    queue::run_serial(|| native::logout())
}
