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

/// Initialize Qonversion and No-Codes on the current platform.
pub(crate) fn initialize(config: &InitConfig) -> Result<(), QonversionError> {
    #[cfg(target_os = "android")]
    {
        android::initialize(config)
    }
    #[cfg(target_os = "ios")]
    {
        ios::initialize(config)
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        unsupported::initialize(config)
    }
}

/// Present a No-Codes screen by context key on the current platform.
pub(crate) fn show_screen(context_key: &str) -> Result<(), QonversionError> {
    #[cfg(target_os = "android")]
    {
        android::show_screen(context_key)
    }
    #[cfg(target_os = "ios")]
    {
        ios::show_screen(context_key)
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        unsupported::show_screen(context_key)
    }
}
