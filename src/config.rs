//! Initialization configuration.

/// Qonversion environment. Use [`Sandbox`](Environment::Sandbox) for store
/// sandbox testers; use [`Production`](Environment::Production) for App Store /
/// Play Store releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Sandbox,
    Production,
}

/// How Qonversion should launch. Subscription Management Mode is required for
/// No-Codes purchase handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    SubscriptionManagement,
}

/// Configuration for [`crate::initialize`].
///
/// The app supplies the project key — never hardcode it in this crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitConfig {
    /// Project key from the Qonversion dashboard.
    pub project_key: String,
    /// Sandbox vs production.
    pub environment: Environment,
    /// Launch mode (Subscription Management for No-Codes purchases).
    pub launch_mode: LaunchMode,
}

impl InitConfig {
    /// Validate required fields before calling native code.
    pub(crate) fn validate(&self) -> Result<(), crate::error::QonversionError> {
        if self.project_key.trim().is_empty() {
            return Err(crate::error::QonversionError::InvalidConfig(
                "project_key must not be empty".into(),
            ));
        }
        Ok(())
    }
}
