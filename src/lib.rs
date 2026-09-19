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
mod entitlements;
mod error;
mod helpers;
mod identity;
mod init;
mod load_screen;
mod native;
mod queue;
mod remote_config;
mod screen;

pub use config::{Environment, InitConfig, LaunchMode};
pub use entitlements::{
    check_entitlements, restore, Entitlement, EntitlementRenewState, EntitlementSource,
};
pub use error::QonversionError;
pub use identity::{identify, logout};
pub use init::{initialize, is_initialized};
pub use load_screen::{
    load_screen, LoadedScreen, ScreenVariable, ScreenVariableKind, ScreenVariableValue,
};
pub use queue::{sdk_timeout, set_sdk_timeout, DEFAULT_SDK_TIMEOUT};
pub use remote_config::{
    remote_config, remote_config_default, Experiment, ExperimentGroup, ExperimentGroupType,
    RemoteConfig, RemoteConfigurationAssignmentType, RemoteConfigurationSource,
    RemoteConfigurationSourceType,
};
pub use screen::{
    clear_screen_event_handler, clear_screen_failed_handler, set_screen_event_handler,
    set_screen_failed_handler, show_screen, ScreenActionKind, ScreenEvent,
};

/// Convenient re-exports for application crates.
pub mod prelude {
    pub use crate::{
        check_entitlements, clear_screen_event_handler, clear_screen_failed_handler, identify,
        initialize, is_initialized, load_screen, logout, remote_config, remote_config_default,
        restore, sdk_timeout, set_screen_event_handler, set_screen_failed_handler, set_sdk_timeout,
        show_screen, Entitlement, EntitlementRenewState, EntitlementSource, Environment,
        Experiment, ExperimentGroup, ExperimentGroupType, InitConfig, LaunchMode, LoadedScreen,
        QonversionError, RemoteConfig, RemoteConfigurationAssignmentType,
        RemoteConfigurationSource, RemoteConfigurationSourceType, ScreenActionKind, ScreenEvent,
        ScreenVariable, ScreenVariableKind, ScreenVariableValue, DEFAULT_SDK_TIMEOUT,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_project_key() {
        let _guard = queue::test_lock();
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
    fn initialize_fails_without_native_runtime() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        let err = initialize(InitConfig {
            project_key: "test_project_key".into(),
            environment: Environment::Production,
            launch_mode: LaunchMode::SubscriptionManagement,
        })
        .expect_err("must fail without a native host");
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        assert_eq!(err, QonversionError::UnsupportedPlatform);
        #[cfg(any(target_os = "android", target_os = "ios"))]
        assert!(matches!(err, QonversionError::HostMissing(_)));
        assert!(!is_initialized());
    }

    #[test]
    fn double_init_is_rejected() {
        let _guard = queue::test_lock();
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

    #[test]
    fn show_screen_rejects_empty_context_key() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = show_screen("  ").expect_err("empty context key must fail");
        assert!(matches!(err, QonversionError::InvalidConfig(_)));
    }

    #[test]
    fn show_screen_requires_init() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        let err = show_screen("paywall").expect_err("must require initialize");
        assert_eq!(err, QonversionError::NotInitialized);
    }

    #[test]
    fn show_screen_fails_without_native_runtime() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = show_screen("paywall").expect_err("must fail without a native host");
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        assert_eq!(err, QonversionError::UnsupportedPlatform);
        #[cfg(any(target_os = "android", target_os = "ios"))]
        assert!(matches!(err, QonversionError::HostMissing(_)));
    }

    #[test]
    fn store_unavailable_is_matchable_and_distinct_from_native() {
        let err = QonversionError::StoreUnavailable;
        assert_ne!(
            err,
            QonversionError::Native {
                message: "PlayStoreError".into()
            }
        );
        assert_eq!(err.to_string(), "native store billing is unavailable");
    }

    #[test]
    fn main_thread_is_matchable_and_distinct_from_native() {
        let err = QonversionError::MainThread;
        assert_ne!(
            err,
            QonversionError::Native {
                message: "main thread".into()
            }
        );
        assert_eq!(
            err.to_string(),
            "Qonversion SDK call must not run on the UI thread"
        );
    }

    #[test]
    fn screen_not_found_is_matchable_and_distinct_from_native() {
        let err = QonversionError::ScreenNotFound;
        assert_ne!(
            err,
            QonversionError::Native {
                message: "ScreenNotFound".into()
            }
        );
        assert_eq!(err.to_string(), "No-Codes screen not found for context key");
    }

    #[test]
    fn load_screen_rejects_empty_context_key() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = load_screen("  ").expect_err("empty context key must fail");
        assert!(matches!(err, QonversionError::InvalidConfig(_)));
    }

    #[test]
    fn load_screen_requires_init() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        let err = load_screen("paywall").expect_err("must require initialize");
        assert_eq!(err, QonversionError::NotInitialized);
    }

    #[test]
    fn load_screen_fails_without_native_runtime() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = load_screen("paywall").expect_err("must fail without a native host");
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        assert_eq!(err, QonversionError::UnsupportedPlatform);
        #[cfg(any(target_os = "android", target_os = "ios"))]
        assert!(matches!(err, QonversionError::HostMissing(_)));
    }

    #[test]
    fn identify_rejects_empty_user_id() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = identify("  ").expect_err("empty user id must fail");
        assert!(matches!(err, QonversionError::InvalidConfig(_)));
    }

    #[test]
    fn identify_requires_init() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        let err = identify("user-1").expect_err("must require initialize");
        assert_eq!(err, QonversionError::NotInitialized);
    }

    #[test]
    fn identify_fails_without_native_runtime() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = identify("user-1").expect_err("must fail without a native host");
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        assert_eq!(err, QonversionError::UnsupportedPlatform);
        #[cfg(any(target_os = "android", target_os = "ios"))]
        assert!(matches!(err, QonversionError::HostMissing(_)));
    }

    #[test]
    fn logout_requires_init() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        let err = logout().expect_err("must require initialize");
        assert_eq!(err, QonversionError::NotInitialized);
    }

    #[test]
    fn logout_fails_without_native_runtime() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = logout().expect_err("must fail without a native host");
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        assert_eq!(err, QonversionError::UnsupportedPlatform);
        #[cfg(any(target_os = "android", target_os = "ios"))]
        assert!(matches!(err, QonversionError::HostMissing(_)));
    }

    #[test]
    fn remote_config_rejects_empty_context_key() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = remote_config("  ").expect_err("empty context key must fail");
        assert!(matches!(err, QonversionError::InvalidConfig(_)));
    }

    #[test]
    fn remote_config_requires_init() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        let err = remote_config("paywall").expect_err("must require initialize");
        assert_eq!(err, QonversionError::NotInitialized);
    }

    #[test]
    fn remote_config_fails_without_native_runtime() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = remote_config("paywall").expect_err("must fail without a native host");
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        assert_eq!(err, QonversionError::UnsupportedPlatform);
        #[cfg(any(target_os = "android", target_os = "ios"))]
        assert!(matches!(err, QonversionError::HostMissing(_)));
    }

    #[test]
    fn remote_config_default_requires_init() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        let err = remote_config_default().expect_err("must require initialize");
        assert_eq!(err, QonversionError::NotInitialized);
    }

    #[test]
    fn remote_config_default_fails_without_native_runtime() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = remote_config_default().expect_err("must fail without a native host");
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        assert_eq!(err, QonversionError::UnsupportedPlatform);
        #[cfg(any(target_os = "android", target_os = "ios"))]
        assert!(matches!(err, QonversionError::HostMissing(_)));
    }

    #[test]
    fn check_entitlements_requires_init() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        let err = check_entitlements().expect_err("must require initialize");
        assert_eq!(err, QonversionError::NotInitialized);
    }

    #[test]
    fn check_entitlements_fails_without_native_runtime() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = check_entitlements().expect_err("must fail without a native host");
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        assert_eq!(err, QonversionError::UnsupportedPlatform);
        #[cfg(any(target_os = "android", target_os = "ios"))]
        assert!(matches!(err, QonversionError::HostMissing(_)));
    }

    #[test]
    fn restore_requires_init() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        let err = restore().expect_err("must require initialize");
        assert_eq!(err, QonversionError::NotInitialized);
    }

    #[test]
    fn restore_fails_without_native_runtime() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = restore().expect_err("must fail without a native host");
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        assert_eq!(err, QonversionError::UnsupportedPlatform);
        #[cfg(any(target_os = "android", target_os = "ios"))]
        assert!(matches!(err, QonversionError::HostMissing(_)));
    }
}
