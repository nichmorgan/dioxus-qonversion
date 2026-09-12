//! One-shot SDK initialization.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::config::InitConfig;
use crate::error::QonversionError;
use crate::native;

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Returns whether [`initialize`] has already succeeded.
pub fn is_initialized() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}

/// Initialize Qonversion (Subscription Management Mode) and No-Codes once.
///
/// Must succeed before any other library call. The project key is supplied by
/// the app — never hardcoded in this crate.
///
/// Double-init returns [`QonversionError::AlreadyInitialized`] without calling
/// native code again. Desktop and web return
/// [`QonversionError::UnsupportedPlatform`].
///
/// Uses compare-and-swap to claim the init slot so concurrent callers cannot
/// both reach native init. On failure the flag is released so retries work.
pub fn initialize(config: InitConfig) -> Result<(), QonversionError> {
    if INITIALIZED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(QonversionError::AlreadyInitialized);
    }

    let result = config.validate().and_then(|()| native::initialize(&config));
    if result.is_err() {
        INITIALIZED.store(false, Ordering::Release);
    }
    result
}

#[cfg(test)]
pub(crate) fn reset_initialized_for_test() {
    INITIALIZED.store(false, Ordering::Release);
}

#[cfg(test)]
pub(crate) fn mark_initialized_for_test() {
    INITIALIZED.store(true, Ordering::Release);
}
