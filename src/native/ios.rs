//! iOS: call the ObjC-visible Swift host shim.
//!
//! Dioxus CLI 0.7+ compiles and embeds the ios/ Swift package registered by
//! ios_plugin. A host that is already loaded (an app that links `ios/` itself)
//! is used as-is. Otherwise the manganis loader opens
//! `Frameworks/DioxusSwiftPlugins.framework`.

use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
use std::sync::OnceLock;

use objc::rc::autoreleasepool;
use objc::runtime::{Class, Object, BOOL, NO};
use objc::{class, msg_send, sel, sel_impl};

use crate::config::{Environment, InitConfig};
use crate::error::QonversionError;

const HOST_CLASS_NAME: &str = "DioxusQonversionHost";

pub(crate) fn initialize(config: &InitConfig) -> Result<(), QonversionError> {
    autoreleasepool(|| {
        let host = host_class()?;
        if !supports_init(host) {
            return Err(QonversionError::HostMissing(
                "DioxusQonversionHost does not accept screen and deferred-purchase callbacks. Rebuild the iOS app with the matching Swift host.".into(),
            ));
        }
        let project_key = nsstring(&config.project_key)?;
        let sandbox = matches!(config.environment, Environment::Sandbox);
        // Static functions stay valid for the life of the Rust image. The objc
        // crate's message encoder accepts pointers, not fn items.
        let failed: extern "C" fn(i32, *const c_char) =
            crate::screen::dioxus_qonversion_notify_screen_failed;
        let event: extern "C" fn(*const c_char) =
            crate::screen::dioxus_qonversion_notify_screen_event;
        let entitlements: extern "C" fn(*const c_char) =
            crate::screen::dioxus_qonversion_notify_entitlements;

        let err: *mut Object = unsafe {
            msg_send![
                host,
                initializeWithProjectKey: project_key
                sandbox: sandbox as u8
                screenFailed: failed as *const c_void
                screenEvent: event as *const c_void
                entitlementsUpdated: entitlements as *const c_void
            ]
        };
        map_host_result(err)
    })
}

fn supports_init(host: &Class) -> bool {
    let selector =
        sel!(initializeWithProjectKey:sandbox:screenFailed:screenEvent:entitlementsUpdated:);
    let supported: BOOL = unsafe { msg_send![host, respondsToSelector: selector] };
    supported != NO
}

pub(crate) fn show_screen(context_key: &str) -> Result<(), QonversionError> {
    autoreleasepool(|| {
        let host = host_class()?;
        let key = nsstring(context_key)?;
        let err: *mut Object = unsafe { msg_send![host, showScreenWithContextKey: key] };
        map_host_result(err)
    })
}

pub(crate) fn load_screen(context_key: &str) -> Result<String, QonversionError> {
    autoreleasepool(|| {
        let host = host_class()?;
        let key = nsstring(context_key)?;
        let timeout_ms = crate::queue::timeout_ms();
        let envelope: *mut Object =
            unsafe { msg_send![host, loadScreenWithContextKey: key timeoutMs: timeout_ms] };
        nsstring_to_rust(envelope).ok_or_else(|| QonversionError::Native {
            message: "DioxusQonversionHost.loadScreenWithContextKey returned nil".into(),
        })
    })
}

pub(crate) fn identify(user_id: &str) -> Result<(), QonversionError> {
    autoreleasepool(|| {
        let host = host_class()?;
        let id = nsstring(user_id)?;
        let timeout_ms = crate::queue::timeout_ms();
        let err: *mut Object =
            unsafe { msg_send![host, identifyWithUserId: id timeoutMs: timeout_ms] };
        map_host_result(err)
    })
}

pub(crate) fn logout() -> Result<(), QonversionError> {
    autoreleasepool(|| {
        let host = host_class()?;
        let timeout_ms = crate::queue::timeout_ms();
        let err: *mut Object = unsafe { msg_send![host, logoutWithTimeoutMs: timeout_ms] };
        map_host_result(err)
    })
}

pub(crate) fn remote_config(context_key: Option<&str>) -> Result<String, QonversionError> {
    autoreleasepool(|| {
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
    })
}

pub(crate) fn check_entitlements() -> Result<String, QonversionError> {
    autoreleasepool(|| {
        let host = host_class()?;
        let timeout_ms = crate::queue::timeout_ms();
        let envelope: *mut Object =
            unsafe { msg_send![host, checkEntitlementsWithTimeoutMs: timeout_ms] };
        require_host_envelope(
            envelope,
            "DioxusQonversionHost.checkEntitlementsWithTimeoutMs",
        )
    })
}

pub(crate) fn restore() -> Result<String, QonversionError> {
    autoreleasepool(|| {
        let host = host_class()?;
        let timeout_ms = crate::queue::timeout_ms();
        let envelope: *mut Object = unsafe { msg_send![host, restoreWithTimeoutMs: timeout_ms] };
        require_host_envelope(envelope, "DioxusQonversionHost.restoreWithTimeoutMs")
    })
}

pub(crate) fn is_main_thread() -> bool {
    extern "C" {
        fn pthread_main_np() -> i32;
    }
    unsafe { pthread_main_np() != 0 }
}

fn host_class() -> Result<&'static Class, QonversionError> {
    // An app that links the host itself already has the class registered.
    if let Some(host) = Class::get(HOST_CLASS_NAME) {
        return Ok(host);
    }
    load_bundled_host()?;
    Class::get(HOST_CLASS_NAME).ok_or_else(|| {
        QonversionError::HostMissing(format!(
            "{HOST_CLASS_NAME} was not registered after loading DioxusSwiftPlugins.framework."
        ))
    })
}

/// Open dx's umbrella framework once, via the loader manganis generates.
/// dx 0.7.10 installs only that framework, so there is no per-product fallback.
fn load_bundled_host() -> Result<(), QonversionError> {
    static LOADED: OnceLock<Result<(), String>> = OnceLock::new();
    match LOADED.get_or_init(|| {
        super::ios_plugin::DioxusQonversionHost::new()
            .map(drop)
            .map_err(str::to_string)
    }) {
        Ok(()) => Ok(()),
        Err(error) => Err(QonversionError::HostMissing(format!(
            "{HOST_CLASS_NAME} not found ({error}). Build with Dioxus CLI 0.7+ so ios/ is embedded as Frameworks/DioxusSwiftPlugins.framework. Without dx, link the local ios/ package. dx 0.7.10 embeds only one Swift plugin framework."
        ))),
    }
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
        return Err(QonversionError::Native {
            message: "failed to allocate NSString".into(),
        });
    }
    // The caller copies any returned string before this pool drains.
    let _: *mut Object = unsafe { msg_send![ns_string, autorelease] };
    Ok(ns_string)
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
