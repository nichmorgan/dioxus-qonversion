//! iOS: call the ObjC-visible Swift host shim.
//!
//! Dioxus CLI 0.7+ compiles and embeds the ios/ Swift package registered by
//! ios_plugin. Load the resulting framework before looking up its ObjC class.
//! Apps that already compile/link the host themselves keep working unchanged.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::os::unix::ffi::OsStrExt;
use std::sync::OnceLock;

use objc::runtime::{Class, Object, BOOL, NO};
use objc::{class, msg_send, sel, sel_impl};

use crate::config::{Environment, InitConfig};
use crate::error::QonversionError;

const HOST_CLASS_NAME: &str = "DioxusQonversionHost";

pub(crate) fn initialize(config: &InitConfig) -> Result<(), QonversionError> {
    let host = host_class()?;
    register_screen_callbacks(host)?;
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

/// Pass real C function pointers before SDK initialization can emit events.
/// Taking these addresses retains the Rust functions without requiring the
/// separately linked Swift framework to import symbols from the executable.
fn register_screen_callbacks(host: &Class) -> Result<(), QonversionError> {
    let supported: BOOL = unsafe {
        msg_send![host, respondsToSelector: sel!(registerScreenCallbacksWithFailed:event:)]
    };
    if supported == NO {
        return Err(QonversionError::HostMissing(
            "DioxusQonversionHost does not support callback registration. Rebuild the iOS app with the matching Swift host.".into(),
        ));
    }

    // Static functions (not capturing closures) remain valid for the lifetime
    // of the Rust image. The host framework is also kept loaded by the loader.
    let failed: extern "C" fn(i32, *const c_char) =
        crate::screen::dioxus_qonversion_notify_screen_failed;
    let event: extern "C" fn(*const c_char) = crate::screen::dioxus_qonversion_notify_screen_event;
    let error: *mut Object = unsafe {
        msg_send![
            host,
            registerScreenCallbacksWithFailed: failed as *const c_void
            event: event as *const c_void
        ]
    };
    map_host_result(error)
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
    extern "C" {
        fn pthread_main_np() -> i32;
    }
    unsafe { pthread_main_np() != 0 }
}

fn host_class() -> Result<&'static Class, QonversionError> {
    // Preserve manual/static integration and hosts loaded by another plugin.
    if let Some(host) = Class::get(HOST_CLASS_NAME) {
        return Ok(host);
    }

    let load_error = load_bundled_swift_host();
    Class::get(HOST_CLASS_NAME).ok_or_else(|| {
        let detail = load_error.map(|error| format!(" ({error})")).unwrap_or_default();
        QonversionError::HostMissing(format!(
            "{HOST_CLASS_NAME} not found{detail}. Build with Dioxus CLI 0.7+ so ios/ (SwiftPM product {HOST_CLASS_NAME}) is compiled and embedded. Without dx, compile the existing Swift host into the app or link the local ios/ package."
        ))
    })
}

/// Load the bundled image once and keep it loaded for the process lifetime.
/// Like dioxus-firebase, try both dx's umbrella name and the product name.
/// Cache the diagnostic as well; host_class always checks for an externally
/// loaded class first, even after a prior loading failure.
fn load_bundled_swift_host() -> Option<String> {
    #[link(name = "System")]
    extern "C" {
        fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    }

    const RTLD_NOW: c_int = 0x2;
    const RTLD_GLOBAL: c_int = 0x8;
    static LOADED: OnceLock<Option<String>> = OnceLock::new();

    LOADED
        .get_or_init(|| {
            let executable = match std::env::current_exe() {
                Ok(path) => path,
                Err(error) => return Some(format!("cannot locate app executable: {error}")),
            };
            let Some(app_dir) = executable.parent() else {
                return Some("app executable has no parent directory".into());
            };
            let frameworks = app_dir.join("Frameworks");
            let mut diagnostics = Vec::new();

            for name in ["DioxusSwiftPlugins", HOST_CLASS_NAME] {
                let path = frameworks.join(format!("{name}.framework/{name}"));
                if !path.is_file() {
                    diagnostics.push(format!("framework not found: {}", path.display()));
                    continue;
                }
                let path_cstr = match CString::new(path.as_os_str().as_bytes()) {
                    Ok(path) => path,
                    Err(_) => {
                        diagnostics
                            .push(format!("framework path contains NUL: {}", path.display()));
                        continue;
                    }
                };
                let handle = unsafe { dlopen(path_cstr.as_ptr(), RTLD_NOW | RTLD_GLOBAL) };
                if handle.is_null() {
                    diagnostics.push(format!(
                        "dlopen {} failed: {}",
                        path.display(),
                        dlerror_message()
                    ));
                    continue;
                }
                // Do not dlclose: Objective-C classes and SDK callbacks remain in use.
                if Class::get(HOST_CLASS_NAME).is_some() {
                    return None;
                }
                diagnostics.push(format!(
                    "loaded {} but {HOST_CLASS_NAME} was not registered",
                    path.display()
                ));
            }
            Some(diagnostics.join("; "))
        })
        .clone()
}

fn dlerror_message() -> String {
    #[link(name = "System")]
    extern "C" {
        fn dlerror() -> *const c_char;
    }
    let error = unsafe { dlerror() };
    if error.is_null() {
        "no dynamic loader diagnostic available".into()
    } else {
        unsafe { CStr::from_ptr(error) }
            .to_string_lossy()
            .into_owned()
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
