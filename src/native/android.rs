//! Android: call the Kotlin host via JNI.
//!
//! The app must:
//! 1. Depend on `io.qonversion:no-codes:1.+` (pulls in the Qonversion SDK).
//! 2. Compile [`android/DioxusQonversionHost.kt`](../../../android/DioxusQonversionHost.kt)
//!    into the Dioxus Android target.
//!
//! Context comes from `ndk_context` (initialized by Dioxus/wry). Host class loading
//! uses the Activity [`ClassLoader`] when bare `FindClass` fails off the UI thread.

use jni::objects::{JClass, JClassLoader, JObject, JString, JValue};
use jni::strings::JNIStr;
use jni::sys::{jboolean, jobject};
use jni::{jni_sig, jni_str, native_method, Env, JavaVM, NativeMethod};

use crate::config::{Environment, InitConfig};
use crate::error::QonversionError;

const HOST_CLASS: &JNIStr = jni_str!("io/dioxus/qonversion/DioxusQonversionHost");

/// Must match `DioxusQonversionHost.SKIP_PREFLIGHT_MAIN_THREAD`.
const SKIP_PREFLIGHT_MAIN_THREAD: &str = "SKIP_PREFLIGHT_MAIN_THREAD";

const NOTIFY_SCREEN_FAILED: NativeMethod = native_method! {
    static fn notify_screen_failed(store_unavailable: jboolean, message: JString),
    name = "notifyScreenFailed",
};

const NOTIFY_SCREEN_EVENT: NativeMethod = native_method! {
    static fn notify_screen_event(json: JString),
    name = "notifyScreenEvent",
};

impl From<jni::errors::Error> for QonversionError {
    fn from(error: jni::errors::Error) -> Self {
        Self::Native {
            message: error.to_string(),
        }
    }
}

pub(crate) fn initialize(config: &InitConfig) -> Result<(), QonversionError> {
    let (vm, context_raw) = java_vm_and_context()?;
    let project_key = config.project_key.clone();
    let sandbox = matches!(config.environment, Environment::Sandbox);

    vm.attach_current_thread(|env| {
        let context = unsafe { JObject::from_raw(env, context_raw) };
        let host = find_host_class(env, &context)?;
        let key = env
            .new_string(&project_key)
            .map_err(|e| QonversionError::Native {
                message: format!("failed to create project key string: {e}"),
            })?;

        register_screen_native_methods(env, &host)?;

        let err = env
            .call_static_method(
                &host,
                jni_str!("initialize"),
                jni_sig!("(Landroid/content/Context;Ljava/lang/String;Z)Ljava/lang/String;"),
                &[
                    JValue::Object(&context),
                    JValue::Object(&key),
                    JValue::Bool(sandbox),
                ],
            )
            .map_err(|e| map_exception(env, e, "DioxusQonversionHost.initialize"))?
            .l()?;

        map_host_result(env, err)
    })
}

pub(crate) fn show_screen(context_key: &str) -> Result<(), QonversionError> {
    let (vm, context_raw) = java_vm_and_context()?;
    let context_key = context_key.to_string();

    vm.attach_current_thread(|env| {
        let context = unsafe { JObject::from_raw(env, context_raw) };
        let activity = as_activity(env, &context)?;
        let host = find_host_class(env, &context)?;

        let preflight = env
            .call_static_method(
                &host,
                jni_str!("ensureStoreAvailable"),
                jni_sig!("(Landroid/content/Context;)Ljava/lang/String;"),
                &[JValue::Object(&context)],
            )
            .map_err(|e| map_exception(env, e, "DioxusQonversionHost.ensureStoreAvailable"))?
            .l()?;
        map_store_preflight(env, preflight)?;

        let key = env
            .new_string(&context_key)
            .map_err(|e| QonversionError::Native {
                message: format!("failed to create context key string: {e}"),
            })?;

        let err = env
            .call_static_method(
                &host,
                jni_str!("showScreen"),
                jni_sig!("(Landroid/app/Activity;Ljava/lang/String;)Ljava/lang/String;"),
                &[JValue::Object(&activity), JValue::Object(&key)],
            )
            .map_err(|e| map_exception(env, e, "DioxusQonversionHost.showScreen"))?
            .l()?;

        map_host_result(env, err)
    })
}

pub(crate) fn load_screen(context_key: &str) -> Result<String, QonversionError> {
    let (vm, context_raw) = java_vm_and_context()?;
    let context_key = context_key.to_string();

    vm.attach_current_thread(|env| {
        let context = unsafe { JObject::from_raw(env, context_raw) };
        let host = find_host_class(env, &context)?;
        let key = env
            .new_string(&context_key)
            .map_err(|e| QonversionError::Native {
                message: format!("failed to create context key string: {e}"),
            })?;

        let envelope = env
            .call_static_method(
                &host,
                jni_str!("loadScreen"),
                jni_sig!("(Ljava/lang/String;)Ljava/lang/String;"),
                &[JValue::Object(&key)],
            )
            .map_err(|e| map_exception(env, e, "DioxusQonversionHost.loadScreen"))?
            .l()?;

        required_jstring(env, envelope, "DioxusQonversionHost.loadScreen")
    })
}

pub(crate) fn identify(user_id: &str) -> Result<(), QonversionError> {
    let (vm, context_raw) = java_vm_and_context()?;
    let user_id = user_id.to_string();

    vm.attach_current_thread(|env| {
        let context = unsafe { JObject::from_raw(env, context_raw) };
        let host = find_host_class(env, &context)?;
        let id = env
            .new_string(&user_id)
            .map_err(|e| QonversionError::Native {
                message: format!("failed to create user id string: {e}"),
            })?;

        let err = env
            .call_static_method(
                &host,
                jni_str!("identify"),
                jni_sig!("(Ljava/lang/String;)Ljava/lang/String;"),
                &[JValue::Object(&id)],
            )
            .map_err(|e| map_exception(env, e, "DioxusQonversionHost.identify"))?
            .l()?;

        map_host_result(env, err)
    })
}

pub(crate) fn logout() -> Result<(), QonversionError> {
    let (vm, context_raw) = java_vm_and_context()?;

    vm.attach_current_thread(|env| {
        let context = unsafe { JObject::from_raw(env, context_raw) };
        let host = find_host_class(env, &context)?;

        let err = env
            .call_static_method(
                &host,
                jni_str!("logout"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )
            .map_err(|e| map_exception(env, e, "DioxusQonversionHost.logout"))?
            .l()?;

        map_host_result(env, err)
    })
}

pub(crate) fn remote_config(context_key: Option<&str>) -> Result<String, QonversionError> {
    let (vm, context_raw) = java_vm_and_context()?;
    let context_key = context_key.map(str::to_string);

    vm.attach_current_thread(|env| {
        let context = unsafe { JObject::from_raw(env, context_raw) };
        let host = find_host_class(env, &context)?;
        let null_key = JObject::null();
        let key;
        let key_arg = if let Some(ref context_key) = context_key {
            key = env
                .new_string(context_key)
                .map_err(|e| QonversionError::Native {
                    message: format!("failed to create context key string: {e}"),
                })?;
            JValue::Object(&key)
        } else {
            JValue::Object(&null_key)
        };

        let envelope = env
            .call_static_method(
                &host,
                jni_str!("remoteConfig"),
                jni_sig!("(Ljava/lang/String;)Ljava/lang/String;"),
                &[key_arg],
            )
            .map_err(|e| map_exception(env, e, "DioxusQonversionHost.remoteConfig"))?
            .l()?;

        required_jstring(env, envelope, "DioxusQonversionHost.remoteConfig")
    })
}

fn java_vm_and_context() -> Result<(JavaVM, jobject), QonversionError> {
    let android_ctx = std::panic::catch_unwind(ndk_context::android_context)
        .ok()
        .filter(|ctx| !ctx.context().is_null() && !ctx.vm().is_null())
        .ok_or_else(|| {
            QonversionError::HostMissing(
                "ndk_context is not initialized (is this a Dioxus Android app?)".into(),
            )
        })?;
    let vm = unsafe { JavaVM::from_raw(android_ctx.vm().cast()) };
    Ok((vm, android_ctx.context() as jobject))
}

fn find_host_class<'a>(
    env: &mut Env<'a>,
    context: &JObject<'_>,
) -> Result<JClass<'a>, QonversionError> {
    match env.find_class(HOST_CLASS) {
        Ok(class) => Ok(class),
        Err(_) => {
            env.exception_clear();
            let loader = activity_class_loader(env, context)?;
            let class_name = env
                .new_string("io.dioxus.qonversion.DioxusQonversionHost")
                .map_err(|e| QonversionError::Native {
                    message: format!("failed to create class name string: {e}"),
                })?;
            let class = env
                .call_method(
                    &loader,
                    jni_str!("loadClass"),
                    jni_sig!("(Ljava/lang/String;)Ljava/lang/Class;"),
                    &[JValue::Object(&class_name)],
                )
                .map_err(|e| {
                    env.exception_clear();
                    QonversionError::HostMissing(format!(
                        "DioxusQonversionHost not found ({e}). Compile android/DioxusQonversionHost.kt into the Android app and add `implementation 'io.qonversion:no-codes:1.+'`."
                    ))
                })?
                .l()
                .map_err(|e| QonversionError::Native {
                    message: format!("loadClass returned unexpected type: {e}"),
                })?;
            env.cast_local::<JClass>(class)
                .map_err(|e| QonversionError::Native {
                    message: format!("failed to cast loaded host class: {e}"),
                })
        }
    }
}

fn as_activity<'a>(
    env: &mut Env<'a>,
    context: &JObject<'_>,
) -> Result<JObject<'a>, QonversionError> {
    let activity_class = env
        .find_class(jni_str!("android/app/Activity"))
        .map_err(|e| QonversionError::Native {
            message: format!("android.app.Activity not found: {e}"),
        })?;
    if !env.is_instance_of(context, &activity_class)? {
        return Err(QonversionError::HostMissing(
            "ndk_context is not an Activity (required to present No-Codes screens)".into(),
        ));
    }
    // Re-wrap the same jobject as a local for the caller.
    Ok(unsafe { JObject::from_raw(env, context.as_raw()) })
}

fn activity_class_loader<'a>(
    env: &mut Env<'a>,
    context: &JObject<'_>,
) -> Result<JClassLoader<'a>, QonversionError> {
    let loader = env
        .call_method(
            context,
            jni_str!("getClassLoader"),
            jni_sig!("()Ljava/lang/ClassLoader;"),
            &[],
        )
        .map_err(|e| map_exception(env, e, "Context.getClassLoader"))?
        .l()
        .map_err(|e| QonversionError::Native {
            message: format!("getClassLoader returned unexpected type: {e}"),
        })?;
    env.cast_local::<JClassLoader>(loader)
        .map_err(|e| QonversionError::Native {
            message: format!("failed to cast ClassLoader: {e}"),
        })
}

fn register_screen_native_methods(
    env: &mut Env<'_>,
    host: &JClass<'_>,
) -> Result<(), QonversionError> {
    unsafe { env.register_native_methods(host, &[NOTIFY_SCREEN_FAILED, NOTIFY_SCREEN_EVENT]) }
        .map_err(|e| QonversionError::Native {
            message: format!("failed to register screen native methods: {e}"),
        })
}

fn notify_screen_failed<'local>(
    env: &mut Env<'local>,
    _class: JClass<'local>,
    store_unavailable: jboolean,
    message: JString<'local>,
) -> Result<(), jni::errors::Error> {
    let message = message.try_to_string(env).unwrap_or_default();
    crate::screen::dispatch_screen_failed(crate::screen::screen_failed_error(
        store_unavailable,
        message,
    ));
    Ok(())
}

fn notify_screen_event<'local>(
    env: &mut Env<'local>,
    _class: JClass<'local>,
    json: JString<'local>,
) -> Result<(), jni::errors::Error> {
    let json = json.try_to_string(env).unwrap_or_default();
    if let Some(event) = crate::screen::parse_event_envelope(&json) {
        crate::screen::dispatch_screen_event(event);
    }
    Ok(())
}

fn map_store_preflight(env: &mut Env<'_>, err: JObject<'_>) -> Result<(), QonversionError> {
    match host_string(
        env,
        err,
        "store preflight result was not a String",
        "unknown store preflight error",
    )? {
        None => Ok(()),
        Some(message) if message == SKIP_PREFLIGHT_MAIN_THREAD => Ok(()),
        Some(_) => Err(QonversionError::StoreUnavailable),
    }
}

fn map_host_result(env: &mut Env<'_>, err: JObject<'_>) -> Result<(), QonversionError> {
    match host_string(
        env,
        err,
        "host error was not a String",
        "unknown native error",
    )? {
        None => Ok(()),
        Some(message) => Err(QonversionError::Native { message }),
    }
}

fn host_string(
    env: &mut Env<'_>,
    obj: JObject<'_>,
    not_a_string: &str,
    utf8_fallback: &str,
) -> Result<Option<String>, QonversionError> {
    if obj.is_null() {
        return Ok(None);
    }
    let jstring = env
        .cast_local::<JString>(obj)
        .map_err(|e| QonversionError::Native {
            message: format!("{not_a_string}: {e}"),
        })?;
    Ok(Some(
        jstring
            .try_to_string(env)
            .unwrap_or_else(|_| utf8_fallback.into()),
    ))
}

fn required_jstring(
    env: &mut Env<'_>,
    obj: JObject<'_>,
    what: &str,
) -> Result<String, QonversionError> {
    if obj.is_null() {
        return Err(QonversionError::Native {
            message: format!("{what} returned null"),
        });
    }
    let jstring = env
        .cast_local::<JString>(obj)
        .map_err(|e| QonversionError::Native {
            message: format!("{what} did not return a String: {e}"),
        })?;
    jstring
        .try_to_string(env)
        .map_err(|e| QonversionError::Native {
            message: format!("failed to read {what} string: {e}"),
        })
}

fn map_exception(env: &mut Env<'_>, err: jni::errors::Error, what: &str) -> QonversionError {
    if env.exception_check() {
        let message = describe_exception(env).unwrap_or_else(|| err.to_string());
        env.exception_clear();
        return QonversionError::Native {
            message: format!("{what}: {message}"),
        };
    }
    QonversionError::Native {
        message: format!("{what}: {err}"),
    }
}

fn describe_exception(env: &mut Env<'_>) -> Option<String> {
    let throwable = env.exception_occurred()?;
    env.exception_clear();
    let message = env
        .call_method(
            &throwable,
            jni_str!("toString"),
            jni_sig!("()Ljava/lang/String;"),
            &[],
        )
        .ok()?
        .l()
        .ok()?;
    let jstring = env.cast_local::<JString>(message).ok()?;
    jstring.try_to_string(env).ok()
}
