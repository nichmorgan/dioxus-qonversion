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
mod user_properties;

pub use config::{Environment, InitConfig, LaunchMode};
pub use entitlements::{
    check_entitlements, check_entitlements_async, restore, restore_async, Entitlement,
    EntitlementRenewState, EntitlementSource,
};
pub use error::QonversionError;
pub use identity::{identify, identify_async, logout, logout_async};
pub use init::{initialize, is_initialized};
pub use load_screen::{
    load_screen, load_screen_async, LoadedScreen, ScreenVariable, ScreenVariableKind,
    ScreenVariableValue,
};
pub use queue::{sdk_timeout, set_sdk_timeout, DEFAULT_SDK_TIMEOUT};
pub use remote_config::{
    remote_config, remote_config_async, remote_config_default, remote_config_default_async,
    Experiment, ExperimentGroup, ExperimentGroupType, RemoteConfig,
    RemoteConfigurationAssignmentType, RemoteConfigurationSource, RemoteConfigurationSourceType,
};
pub use screen::{
    clear_entitlements_updated_handler, clear_screen_event_handler, clear_screen_failed_handler,
    set_entitlements_updated_handler, set_screen_event_handler, set_screen_failed_handler,
    show_screen, ScreenActionKind, ScreenEvent,
};
pub use user_properties::{
    set_custom_user_property, set_custom_user_property_async, set_user_property,
    set_user_property_async, UserPropertyKey,
};

/// Convenient re-exports for application crates.
pub mod prelude {
    pub use crate::{
        check_entitlements, check_entitlements_async, clear_entitlements_updated_handler,
        clear_screen_event_handler, clear_screen_failed_handler, identify, identify_async,
        initialize, is_initialized, load_screen, load_screen_async, logout, logout_async,
        remote_config, remote_config_async, remote_config_default, remote_config_default_async,
        restore, restore_async, sdk_timeout, set_custom_user_property,
        set_custom_user_property_async, set_entitlements_updated_handler, set_screen_event_handler,
        set_screen_failed_handler, set_sdk_timeout, set_user_property, set_user_property_async,
        show_screen, Entitlement, EntitlementRenewState, EntitlementSource, Environment,
        Experiment, ExperimentGroup, ExperimentGroupType, InitConfig, LaunchMode, LoadedScreen,
        QonversionError, RemoteConfig, RemoteConfigurationAssignmentType,
        RemoteConfigurationSource, RemoteConfigurationSourceType, ScreenActionKind, ScreenEvent,
        ScreenVariable, ScreenVariableKind, ScreenVariableValue, UserPropertyKey,
        DEFAULT_SDK_TIMEOUT,
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
        let err = queue::off_main(|| load_screen("paywall"))
            .expect_err("must fail without a native host");
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
        let err =
            queue::off_main(|| identify("user-1")).expect_err("must fail without a native host");
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
        let err = queue::off_main(logout).expect_err("must fail without a native host");
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        assert_eq!(err, QonversionError::UnsupportedPlatform);
        #[cfg(any(target_os = "android", target_os = "ios"))]
        assert!(matches!(err, QonversionError::HostMissing(_)));
    }

    #[test]
    fn set_user_property_rejects_empty_value() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = set_user_property(UserPropertyKey::Email, "  ").expect_err("empty value");
        assert!(matches!(err, QonversionError::InvalidConfig(_)));
    }

    #[test]
    fn set_custom_user_property_rejects_bad_key() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = set_custom_user_property("_q_email", "x").expect_err("reserved key");
        assert!(matches!(err, QonversionError::InvalidConfig(_)));
        let err = set_custom_user_property("bad key", "x").expect_err("illegal chars");
        assert!(matches!(err, QonversionError::InvalidConfig(_)));
    }

    #[test]
    fn set_user_property_requires_init() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        let err = set_user_property(UserPropertyKey::Email, "a@b.c")
            .expect_err("must require initialize");
        assert_eq!(err, QonversionError::NotInitialized);
    }

    #[test]
    fn set_user_property_fails_without_native_runtime() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = queue::off_main(|| set_user_property(UserPropertyKey::Email, "a@b.c"))
            .expect_err("must fail without a native host");
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        assert_eq!(err, QonversionError::UnsupportedPlatform);
        #[cfg(any(target_os = "android", target_os = "ios"))]
        assert!(matches!(err, QonversionError::HostMissing(_)));
    }

    #[test]
    fn set_custom_user_property_fails_without_native_runtime() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = queue::off_main(|| set_custom_user_property("liked_app", "yes"))
            .expect_err("must fail without a native host");
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
    fn remote_config_async_requires_init_on_the_main_thread() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        struct RestoreMain;
        impl Drop for RestoreMain {
            fn drop(&mut self) {
                native::set_simulate_main_thread(false);
            }
        }
        let _restore = RestoreMain;
        native::set_simulate_main_thread(true);
        let err = queue::block_on(remote_config_async("paywall"))
            .expect_err("async call must require initialize");
        assert_eq!(err, QonversionError::NotInitialized);
    }

    #[test]
    fn remote_config_fails_without_native_runtime() {
        let _guard = queue::test_lock();
        init::reset_initialized_for_test();
        init::mark_initialized_for_test();
        let err = queue::off_main(|| remote_config("paywall"))
            .expect_err("must fail without a native host");
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
        let err =
            queue::off_main(remote_config_default).expect_err("must fail without a native host");
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
        let err = queue::off_main(check_entitlements).expect_err("must fail without a native host");
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
        let err = queue::off_main(restore).expect_err("must fail without a native host");
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        assert_eq!(err, QonversionError::UnsupportedPlatform);
        #[cfg(any(target_os = "android", target_os = "ios"))]
        assert!(matches!(err, QonversionError::HostMissing(_)));
    }
}
