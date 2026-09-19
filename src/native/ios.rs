//! iOS: call the ObjC-visible Swift host shim.
//!
//! The app must:
//! 1. Add the Qonversion iOS SDK (SPM, ≥ 6.13.0) which includes No-Codes.
//! 2. Compile [`ios/DioxusQonversionHost.swift`](../../../ios/DioxusQonversionHost.swift)
//!    into the Dioxus iOS target.

use std::ffi::CStr;

use objc::runtime::{Class, Object};
use objc::{class, msg_send, sel, sel_impl};

use crate::config::{Environment, InitConfig};
use crate::error::QonversionError;

pub(crate) fn initialize(config: &InitConfig) -> Result<(), QonversionError> {
    let host = host_class()?;
    let project_key = nsstring(&config.project_key)?;
    let sandbox = matches!(config.environment, Environment::Sandbox);

    let err: *mut Object = unsafe {
        msg_send![
            host,
            initializeWithProjectKey: project_key
            sandbox: sandbox as u8
        ]
    };

    map_host_result(err)
}

pub(crate) fn show_screen(context_key: &str) -> Result<(), QonversionError> {
    let host = host_class()?;
    let key = nsstring(context_key)?;

    let err: *mut Object = unsafe { msg_send![host, showScreenWithContextKey: key] };

    map_host_result(err)
}

pub(crate) fn load_screen(context_key: &str) -> Result<String, QonversionError> {
    let host = host_class()?;
    let key = nsstring(context_key)?;
    let timeout_ms = crate::queue::timeout_ms();

    let envelope: *mut Object =
        unsafe { msg_send![host, loadScreenWithContextKey: key timeoutMs: timeout_ms] };
    nsstring_to_rust(envelope).ok_or_else(|| QonversionError::Native {
        message: "DioxusQonversionHost.loadScreenWithContextKey returned nil".into(),
    })
}

pub(crate) fn identify(user_id: &str) -> Result<(), QonversionError> {
    let host = host_class()?;
    let id = nsstring(user_id)?;
    let timeout_ms = crate::queue::timeout_ms();

    let err: *mut Object = unsafe { msg_send![host, identifyWithUserId: id timeoutMs: timeout_ms] };

    map_host_result(err)
}

pub(crate) fn logout() -> Result<(), QonversionError> {
    let host = host_class()?;
    let timeout_ms = crate::queue::timeout_ms();

    let err: *mut Object = unsafe { msg_send![host, logoutWithTimeoutMs: timeout_ms] };

    map_host_result(err)
}

pub(crate) fn remote_config(context_key: Option<&str>) -> Result<String, QonversionError> {
    let host = host_class()?;
    let key = match context_key {
        Some(key) => nsstring(key)?,
        None => std::ptr::null_mut(),
    };
    let timeout_ms = crate::queue::timeout_ms();

    let envelope: *mut Object =
        unsafe { msg_send![host, remoteConfigWithContextKey: key timeoutMs: timeout_ms] };
    nsstring_to_rust(envelope).ok_or_else(|| QonversionError::Native {
        message: "DioxusQonversionHost.remoteConfigWithContextKey returned nil".into(),
    })
}

pub(crate) fn check_entitlements() -> Result<String, QonversionError> {
    let host = host_class()?;
    let timeout_ms = crate::queue::timeout_ms();
    let envelope: *mut Object =
        unsafe { msg_send![host, checkEntitlementsWithTimeoutMs: timeout_ms] };
    require_host_envelope(
        envelope,
        "DioxusQonversionHost.checkEntitlementsWithTimeoutMs",
    )
}

pub(crate) fn restore() -> Result<String, QonversionError> {
    let host = host_class()?;
    let timeout_ms = crate::queue::timeout_ms();
    let envelope: *mut Object = unsafe { msg_send![host, restoreWithTimeoutMs: timeout_ms] };
    require_host_envelope(envelope, "DioxusQonversionHost.restoreWithTimeoutMs")
}

pub(crate) fn is_main_thread() -> bool {
    let is_main: bool = unsafe { msg_send![class!(NSThread), isMainThread] };
    is_main
}

fn host_class() -> Result<&'static Class, QonversionError> {
    Class::get("DioxusQonversionHost").ok_or_else(|| {
        QonversionError::HostMissing(
            "DioxusQonversionHost not found. Compile ios/DioxusQonversionHost.swift into the iOS app and link the Qonversion SDK (≥ 6.13.0).".into(),
        )
    })
}

fn require_host_envelope(ptr: *mut Object, what: &str) -> Result<String, QonversionError> {
    nsstring_to_rust(ptr).ok_or_else(|| QonversionError::Native {
        message: format!("{what} returned nil"),
    })
}

fn map_host_result(err: *mut Object) -> Result<(), QonversionError> {
    if err.is_null() {
        Ok(())
    } else {
        let message = nsstring_to_rust(err).unwrap_or_else(|| "unknown native error".into());
        Err(crate::helpers::map_host_error_message(message))
    }
}

fn nsstring(s: &str) -> Result<*mut Object, QonversionError> {
    let bytes = s.as_bytes();
    let ns_string: *mut Object = unsafe {
        let alloc: *mut Object = msg_send![class!(NSString), alloc];
        msg_send![
            alloc,
            initWithBytes: bytes.as_ptr()
            length: bytes.len()
            encoding: 4usize // NSUTF8StringEncoding
        ]
    };
    if ns_string.is_null() {
        Err(QonversionError::Native {
            message: "failed to allocate NSString".into(),
        })
    } else {
        Ok(ns_string)
    }
}

fn nsstring_to_rust(s: *mut Object) -> Option<String> {
    if s.is_null() {
        return None;
    }
    let cstr: *const i8 = unsafe { msg_send![s, UTF8String] };
    if cstr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(cstr) }
        .to_str()
        .ok()
        .map(str::to_owned)
}
