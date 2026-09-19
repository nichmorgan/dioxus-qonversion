//! Check and restore Qonversion entitlements.

use std::collections::HashMap;

use serde_json::Value;

use crate::error::QonversionError;
use crate::init;
use crate::native;
use crate::queue;

/// Current entitlement granted to the Qonversion user.
///
/// An empty map from [`check_entitlements`] or [`restore`] is success — the user
/// has no entitlements, not an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entitlement {
    /// Dashboard entitlement id (map key is the same id).
    pub id: String,
    /// `true` means the user currently has access. Not the same as “will renew”.
    pub is_active: bool,
    /// Qonversion product id that unlocked this entitlement, when the SDK provides one.
    pub product_id: Option<String>,
    /// Subscription expiration as milliseconds since Unix epoch (UTC).
    /// `None` for consumable / non-consumable / lifetime access.
    pub expiration_date: Option<i64>,
    /// Renewal state of the associated subscription.
    pub renew_state: EntitlementRenewState,
    /// Store or grant source that activated this entitlement.
    pub source: EntitlementSource,
}

/// Renewal state from the official Entitlement object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntitlementRenewState {
    NonRenewable,
    WillRenew,
    BillingIssue,
    Canceled,
    Unknown,
}

/// How the entitlement was activated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntitlementSource {
    AppStore,
    PlayStore,
    Stripe,
    Manual,
    Unknown,
}

/// Return the current entitlement map from Qonversion.
///
/// Maps to official `checkEntitlements()`. An empty map is success (no
/// entitlements). Runs on the serial SDK queue with
/// [`crate::DEFAULT_SDK_TIMEOUT`]. Blocking wait: call from Dioxus `spawn` /
/// a background thread; the UI thread returns [`QonversionError::MainThread`].
///
/// The app owns which entitlement id means “premium”.
pub fn check_entitlements() -> Result<HashMap<String, Entitlement>, QonversionError> {
    entitlements_from(native::check_entitlements)
}

/// Restore Store-linked purchases and return the refreshed entitlement map.
///
/// Maps to official `restore()`. Does not create a new purchase or charge the
/// user. Same queue, timeout, and threading rules as [`check_entitlements`].
pub fn restore() -> Result<HashMap<String, Entitlement>, QonversionError> {
    entitlements_from(native::restore)
}

fn entitlements_from(
    native: impl FnOnce() -> Result<String, QonversionError> + Send + 'static,
) -> Result<HashMap<String, Entitlement>, QonversionError> {
    init::require_initialized()?;
    queue::run_serial(|| parse_envelope(&native()?))
}

/// Parse the native host JSON envelope into an entitlement map.
pub(crate) fn parse_envelope(json: &str) -> Result<HashMap<String, Entitlement>, QonversionError> {
    let object = crate::helpers::parse_ok_envelope(json, "entitlements", |_| None)?;

    let map = match object.get("entitlements") {
        None | Some(Value::Null) => return Ok(HashMap::new()),
        Some(Value::Object(map)) => map,
        Some(_) => {
            return Err(QonversionError::Native {
                message: "entitlements must be a JSON object".into(),
            });
        }
    };

    let mut entitlements = HashMap::with_capacity(map.len());
    for (key, value) in map {
        entitlements.insert(key.clone(), parse_entitlement(value, key)?);
    }
    Ok(entitlements)
}

fn parse_entitlement(value: &Value, id: &str) -> Result<Entitlement, QonversionError> {
    let object = value.as_object().ok_or_else(|| QonversionError::Native {
        message: "entitlement must be a JSON object".into(),
    })?;

    Ok(Entitlement {
        id: id.to_string(),
        is_active: object
            .get("is_active")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        product_id: object
            .get("product_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string),
        expiration_date: object.get("expiration_date").and_then(Value::as_i64),
        renew_state: parse_renew_state(object.get("renew_state").and_then(Value::as_str)),
        source: parse_source(object.get("source").and_then(Value::as_str)),
    })
}

fn parse_renew_state(raw: Option<&str>) -> EntitlementRenewState {
    match raw.unwrap_or("") {
        "non_renewable" => EntitlementRenewState::NonRenewable,
        "will_renew" => EntitlementRenewState::WillRenew,
        "billing_issue" => EntitlementRenewState::BillingIssue,
        "canceled" | "cancelled" => EntitlementRenewState::Canceled,
        _ => EntitlementRenewState::Unknown,
    }
}

fn parse_source(raw: Option<&str>) -> EntitlementSource {
    match raw.unwrap_or("") {
        "appstore" => EntitlementSource::AppStore,
        "playstore" => EntitlementSource::PlayStore,
        "stripe" => EntitlementSource::Stripe,
        "manual" => EntitlementSource::Manual,
        _ => EntitlementSource::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_map_is_success() {
        let entitlements = parse_envelope(r#"{"ok":true,"entitlements":{}}"#).expect("empty");
        assert!(entitlements.is_empty());
    }

    #[test]
    fn missing_entitlements_is_empty_map() {
        let entitlements = parse_envelope(r#"{"ok":true}"#).expect("missing");
        assert!(entitlements.is_empty());
    }

    #[test]
    fn ok_false_is_native_error() {
        let err = parse_envelope(r#"{"ok":false,"error":"offline"}"#).expect_err("fail");
        assert_eq!(
            err,
            QonversionError::Native {
                message: "offline".into()
            }
        );
    }

    #[test]
    fn timed_out_is_timeout() {
        let err =
            parse_envelope(r#"{"ok":false,"timed_out":true,"error":"late"}"#).expect_err("timeout");
        assert!(matches!(err, QonversionError::Timeout { .. }));
    }

    #[test]
    fn full_entitlement_round_trip() {
        let json = json!({
            "ok": true,
            "entitlements": {
                "premium": {
                    "id": "premium",
                    "is_active": true,
                    "product_id": "monthly",
                    "expiration_date": 1_700_000_000_000i64,
                    "renew_state": "will_renew",
                    "source": "appstore"
                }
            }
        });
        let entitlements = parse_envelope(&json.to_string()).expect("full");
        let premium = entitlements.get("premium").expect("premium");
        assert_eq!(premium.id, "premium");
        assert!(premium.is_active);
        assert_eq!(premium.product_id.as_deref(), Some("monthly"));
        assert_eq!(premium.expiration_date, Some(1_700_000_000_000));
        assert_eq!(premium.renew_state, EntitlementRenewState::WillRenew);
        assert_eq!(premium.source, EntitlementSource::AppStore);
    }

    #[test]
    fn empty_product_and_unknown_enums() {
        let json = json!({
            "ok": true,
            "entitlements": {
                "lifetime": {
                    "id": "lifetime",
                    "is_active": false,
                    "product_id": "",
                    "expiration_date": null,
                    "renew_state": "future",
                    "source": "future"
                }
            }
        });
        let entitlement = parse_envelope(&json.to_string())
            .expect("parse")
            .remove("lifetime")
            .expect("lifetime");
        assert!(entitlement.product_id.is_none());
        assert!(entitlement.expiration_date.is_none());
        assert_eq!(entitlement.renew_state, EntitlementRenewState::Unknown);
        assert_eq!(entitlement.source, EntitlementSource::Unknown);
    }

    #[test]
    fn cancelled_alias() {
        let json = json!({
            "ok": true,
            "entitlements": {
                "plus": {
                    "is_active": true,
                    "renew_state": "cancelled",
                    "source": "playstore"
                }
            }
        });
        let entitlement = parse_envelope(&json.to_string())
            .expect("parse")
            .remove("plus")
            .expect("plus");
        assert_eq!(entitlement.id, "plus");
        assert_eq!(entitlement.renew_state, EntitlementRenewState::Canceled);
        assert_eq!(entitlement.source, EntitlementSource::PlayStore);
    }
}
