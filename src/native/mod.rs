//! Platform dispatch for native Qonversion + No-Codes calls.

use crate::config::InitConfig;
use crate::error::QonversionError;

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
mod android_plugin;
#[cfg(target_os = "ios")]
mod ios;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod unsupported;

#[cfg(target_os = "android")]
use android as sys;
#[cfg(target_os = "ios")]
use ios as sys;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use unsupported as sys;

/// Initialize Qonversion and No-Codes on the current platform.
pub(crate) fn initialize(config: &InitConfig) -> Result<(), QonversionError> {
    sys::initialize(config)
}

/// Present a No-Codes screen by context key on the current platform.
pub(crate) fn show_screen(context_key: &str) -> Result<(), QonversionError> {
    sys::show_screen(context_key)
}

/// Load a No-Codes screen by context key without presenting it.
pub(crate) fn load_screen(context_key: &str) -> Result<String, QonversionError> {
    sys::load_screen(context_key)
}

/// Identify the Qonversion user with a stable app user id.
pub(crate) fn identify(user_id: &str) -> Result<(), QonversionError> {
    sys::identify(user_id)
}

/// Clear the Qonversion user session.
pub(crate) fn logout() -> Result<(), QonversionError> {
    sys::logout()
}

/// Fetch Remote Config. `None` is the dashboard empty context key.
pub(crate) fn remote_config(context_key: Option<&str>) -> Result<String, QonversionError> {
    sys::remote_config(context_key)
}

/// True when the current thread is the Android main looper or iOS main thread.
pub(crate) fn is_main_thread() -> bool {
    sys::is_main_thread()
}
