//! Platform dispatch for native Qonversion + No-Codes calls.

use crate::config::InitConfig;
use crate::error::QonversionError;

#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
mod android_plugin;
#[cfg(target_os = "ios")]
mod ios;
#[cfg(target_os = "ios")]
mod ios_plugin;
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

/// Return the current entitlement map envelope.
pub(crate) fn check_entitlements() -> Result<String, QonversionError> {
    sys::check_entitlements()
}

/// Restore purchases and return the entitlement map envelope.
pub(crate) fn restore() -> Result<String, QonversionError> {
    sys::restore()
}

#[cfg(test)]
static SIMULATE_MAIN_THREAD: AtomicBool = AtomicBool::new(false);

/// True when the current thread is the Android main looper or iOS main thread.
pub(crate) fn is_main_thread() -> bool {
    #[cfg(test)]
    if SIMULATE_MAIN_THREAD.load(Ordering::Relaxed) {
        return true;
    }
    sys::is_main_thread()
}

/// Force [`is_main_thread`] for tests that cannot run on a device UI thread.
#[cfg(test)]
pub(crate) fn set_simulate_main_thread(value: bool) {
    SIMULATE_MAIN_THREAD.store(value, Ordering::Relaxed);
}
