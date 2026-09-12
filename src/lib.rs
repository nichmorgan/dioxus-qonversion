//! Unofficial Qonversion primitives for Dioxus mobile.
//!
//! **Mobile only** (iOS and Android). Web and desktop return
//! [`QonversionError::UnsupportedPlatform`].
//!
//! Native host: JNI on Android, a small ObjC-visible Swift shim on iOS — not UniFFI.
//!
//! The public API is not stable yet. See the
//! [roadmap](https://github.com/nichmorgan/dioxus-qonversion/issues/1).

mod config;
mod error;
mod init;
mod native;

pub use config::{Environment, InitConfig, LaunchMode};
pub use error::QonversionError;
pub use init::{initialize, is_initialized};

/// Convenient re-exports for application crates.
pub mod prelude {
    pub use crate::{
        initialize, is_initialized, Environment, InitConfig, LaunchMode, QonversionError,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_project_key() {
        init::reset_initialized_for_test();
        let err = initialize(InitConfig {
            project_key: "  ".into(),
            environment: Environment::Sandbox,
            launch_mode: LaunchMode::SubscriptionManagement,
        })
        .expect_err("empty key must fail");
        assert!(matches!(err, QonversionError::InvalidConfig(_)));
        assert!(!is_initialized());
    }

    #[test]
    fn unsupported_on_non_mobile() {
        init::reset_initialized_for_test();
        let err = initialize(InitConfig {
            project_key: "test_project_key".into(),
            environment: Environment::Production,
            launch_mode: LaunchMode::SubscriptionManagement,
        })
        .expect_err("desktop/web must fail");
        assert_eq!(err, QonversionError::UnsupportedPlatform);
        assert!(!is_initialized());
    }

    #[test]
    fn double_init_is_rejected() {
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = initialize(InitConfig {
            project_key: "test_project_key".into(),
            environment: Environment::Sandbox,
            launch_mode: LaunchMode::SubscriptionManagement,
        })
        .expect_err("second init must fail");
        assert_eq!(err, QonversionError::AlreadyInitialized);
    }
}
