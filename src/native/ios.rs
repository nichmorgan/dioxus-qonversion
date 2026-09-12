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
    let host = Class::get("DioxusQonversionHost").ok_or_else(|| {
        QonversionError::HostMissing(
            "DioxusQonversionHost not found. Compile ios/DioxusQonversionHost.swift into the iOS app and link the Qonversion SDK (≥ 6.13.0).".into(),
        )
    })?;

    let project_key = nsstring(&config.project_key)?;
    let sandbox = matches!(config.environment, Environment::Sandbox);

    let err: *mut Object = unsafe {
        msg_send![
            host,
            initializeWithProjectKey: project_key
            sandbox: sandbox as u8
        ]
    };

    if err.is_null() {
        Ok(())
    } else {
        let message = nsstring_to_rust(err).unwrap_or_else(|| "unknown native error".into());
        Err(QonversionError::Native { message })
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
            message: "failed to allocate NSString for project key".into(),
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
