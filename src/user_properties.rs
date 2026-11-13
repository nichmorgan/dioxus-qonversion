//! User property primitives (`set_user_property` / `set_custom_user_property`).
//!
//! Official Qonversion [User Properties](https://documentation.qonversion.io/docs/user-properties.md):
//! defined keys plus custom attributes for segmentation, analytics, and integrations.

use crate::error::QonversionError;
use crate::init;
use crate::native;
use crate::queue;

/// Qonversion-defined user property keys (`_q_*` on the server).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UserPropertyKey {
    Email,
    Name,
    KochavaDeviceId,
    AppsFlyerUserId,
    AdjustAdId,
    CustomUserId,
    FacebookAttribution,
    FirebaseAppInstanceId,
    AppSetId,
    AdvertisingId,
    PushWooshUserId,
    PushWooshHwId,
    AppMetricaDeviceId,
    AppMetricaUserProfileId,
    TenjinAnalyticsInstallationId,
}

impl From<UserPropertyKey> for &'static str {
    fn from(key: UserPropertyKey) -> Self {
        match key {
            UserPropertyKey::Email => "email",
            UserPropertyKey::Name => "name",
            UserPropertyKey::KochavaDeviceId => "kochava_device_id",
            UserPropertyKey::AppsFlyerUserId => "appsflyer_user_id",
            UserPropertyKey::AdjustAdId => "adjust_ad_id",
            UserPropertyKey::CustomUserId => "custom_user_id",
            UserPropertyKey::FacebookAttribution => "facebook_attribution",
            UserPropertyKey::FirebaseAppInstanceId => "firebase_app_instance_id",
            UserPropertyKey::AppSetId => "app_set_id",
            UserPropertyKey::AdvertisingId => "advertising_id",
            UserPropertyKey::PushWooshUserId => "pushwoosh_user_id",
            UserPropertyKey::PushWooshHwId => "pushwoosh_hw_id",
            UserPropertyKey::AppMetricaDeviceId => "appmetrica_device_id",
            UserPropertyKey::AppMetricaUserProfileId => "appmetrica_user_profile_id",
            UserPropertyKey::TenjinAnalyticsInstallationId => "tenjin_analytics_installation_id",
        }
    }
}

/// Set a Qonversion-defined user property.
///
/// Call after [`crate::initialize`]. Runs on the serial SDK queue with
/// [`crate::DEFAULT_SDK_TIMEOUT`]. Blocking wait: call from a background
/// thread. The UI thread returns [`QonversionError::MainThread`]. On iOS,
/// Dioxus `spawn` runs on the UI thread — await [`set_user_property_async`]
/// from `spawn`.
///
/// Pending properties flush when presenting a No-Codes screen via
/// [`crate::show_screen`]; official `loadScreen` does not flush them.
pub fn set_user_property(key: UserPropertyKey, value: &str) -> Result<(), QonversionError> {
    let value = crate::helpers::require_non_empty("value", value)?;
    init::require_initialized()?;
    let value = value.to_string();
    queue::run_serial(move || native::set_user_property(key.into(), &value))
}

/// [`set_user_property`] for Dioxus `spawn`. See [`crate::remote_config_async`].
pub async fn set_user_property_async(
    key: UserPropertyKey,
    value: &str,
) -> Result<(), QonversionError> {
    let value = crate::helpers::require_non_empty("value", value)?;
    init::require_initialized()?;
    let value = value.to_string();
    queue::run_serial_async(move || native::set_user_property(key.into(), &value)).await
}

/// Set a custom user property (`key` → `value`).
///
/// Custom keys may use letters, digits, and `_.:-` only. Keys must not start
/// with `_q_` (reserved for defined properties). Same queue / timeout /
/// main-thread rules as [`set_user_property`].
pub fn set_custom_user_property(key: &str, value: &str) -> Result<(), QonversionError> {
    let key = require_custom_property_key(key)?;
    let value = crate::helpers::require_non_empty("value", value)?;
    init::require_initialized()?;
    let key = key.to_string();
    let value = value.to_string();
    queue::run_serial(move || native::set_custom_user_property(&key, &value))
}

/// [`set_custom_user_property`] for Dioxus `spawn`.
pub async fn set_custom_user_property_async(key: &str, value: &str) -> Result<(), QonversionError> {
    let key = require_custom_property_key(key)?;
    let value = crate::helpers::require_non_empty("value", value)?;
    init::require_initialized()?;
    let key = key.to_string();
    let value = value.to_string();
    queue::run_serial_async(move || native::set_custom_user_property(&key, &value)).await
}

fn require_custom_property_key(key: &str) -> Result<&str, QonversionError> {
    let key = crate::helpers::require_non_empty("key", key)?;
    if key.starts_with("_q_") {
        return Err(QonversionError::InvalidConfig(
            "custom property key must not start with _q_".into(),
        ));
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
    {
        return Err(QonversionError::InvalidConfig(
            "custom property key may only contain A-Za-z, digits, and _.:-".into(),
        ));
    }
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_keys_are_stable() {
        assert_eq!(<&'static str>::from(UserPropertyKey::Email), "email");
        assert_eq!(
            <&'static str>::from(UserPropertyKey::TenjinAnalyticsInstallationId),
            "tenjin_analytics_installation_id"
        );
    }

    #[test]
    fn custom_key_rejects_reserved_prefix() {
        let err = require_custom_property_key("_q_email").expect_err("reserved");
        assert!(matches!(err, QonversionError::InvalidConfig(_)));
    }

    #[test]
    fn custom_key_rejects_illegal_chars() {
        let err = require_custom_property_key("liked app").expect_err("space");
        assert!(matches!(err, QonversionError::InvalidConfig(_)));
    }

    #[test]
    fn custom_key_accepts_allowed_charset() {
        assert_eq!(
            require_custom_property_key("liked_app.v1:test-2").unwrap(),
            "liked_app.v1:test-2"
        );
    }
}
