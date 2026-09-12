//! Present a Qonversion No-Codes screen.

use crate::error::QonversionError;
use crate::init;
use crate::native;

/// Present a No-Codes screen by context key.
///
/// Returns when the native SDK has been asked to show the screen, not when
/// the user dismisses it. A missing/invalid key may still return `Ok` and
/// fail later via finished/failed-to-load callbacks (F03).
///
/// Requires a successful [`crate::initialize`] first. Desktop and web return
/// [`QonversionError::UnsupportedPlatform`].
pub fn show_screen(context_key: &str) -> Result<(), QonversionError> {
    if context_key.trim().is_empty() {
        return Err(QonversionError::InvalidConfig(
            "context_key must not be empty".into(),
        ));
    }
    init::require_initialized()?;
    native::show_screen(context_key)
}
