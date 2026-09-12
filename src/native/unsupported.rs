//! Stub for desktop / web — this crate is mobile-only.

use crate::config::InitConfig;
use crate::error::QonversionError;

pub(crate) fn initialize(_config: &InitConfig) -> Result<(), QonversionError> {
    Err(QonversionError::UnsupportedPlatform)
}
