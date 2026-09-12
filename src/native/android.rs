//! Android: call Qonversion + No-Codes via JNI.
//!
//! Requires the app to depend on `io.qonversion:no-codes:1.+` (pulls in the
//! Qonversion SDK). Context comes from `ndk_context` (initialized by Dioxus/wry).

use jni::objects::{JObject, JString, JValue};
use jni::strings::JNIStr;
use jni::{jni_sig, jni_str, Env, JavaVM};

use crate::config::{Environment, InitConfig, LaunchMode};
use crate::error::QonversionError;

impl From<jni::errors::Error> for QonversionError {
    fn from(error: jni::errors::Error) -> Self {
        Self::Native {
            message: error.to_string(),
        }
    }
}

pub(crate) fn initialize(config: &InitConfig) -> Result<(), QonversionError> {
    with_jni(|env, context| {
        initialize_qonversion(env, context, config)?;
        initialize_nocodes(env, context, &config.project_key)?;
        Ok(())
    })
}

pub(crate) fn show_screen(context_key: &str) -> Result<(), QonversionError> {
    with_jni(|env, _context| {
        let key = env
            .new_string(context_key)
            .map_err(|e| QonversionError::Native {
                message: format!("failed to create context key string: {e}"),
            })?;

        let nocodes_class = find_class(env, jni_str!("io/qonversion/nocodes/NoCodes"))?;
        let shared = env
            .call_static_method(
                &nocodes_class,
                jni_str!("getSharedInstance"),
                jni_sig!("()Lio/qonversion/nocodes/NoCodes;"),
                &[],
            )
            .map_err(|e| map_exception(env, e, "NoCodes.getSharedInstance"))?
            .l()
            .map_err(|e| QonversionError::Native {
                message: format!("getSharedInstance returned unexpected type: {e}"),
            })?;

        env.call_method(
            &shared,
            jni_str!("showScreen"),
            jni_sig!("(Ljava/lang/String;)V"),
            &[JValue::Object(&key)],
        )
        .map_err(|e| map_exception(env, e, "NoCodes.showScreen"))?;

        Ok(())
    })
}

fn with_jni<F>(f: F) -> Result<(), QonversionError>
where
    F: for<'a> FnOnce(&mut Env<'a>, &JObject<'a>) -> Result<(), QonversionError>,
{
    let android_ctx = ndk_context::android_context();
    if android_ctx.context().is_null() || android_ctx.vm().is_null() {
        return Err(QonversionError::HostMissing(
            "ndk_context is not initialized (is this a Dioxus Android app?)".into(),
        ));
    }

    let vm = unsafe { JavaVM::from_raw(android_ctx.vm().cast()) };

    vm.attach_current_thread(|env| {
        let context =
            unsafe { JObject::from_raw(env, android_ctx.context() as jni::sys::jobject) };
        f(env, &context)
    })
}

fn initialize_qonversion(
    env: &mut Env<'_>,
    context: &JObject<'_>,
    config: &InitConfig,
) -> Result<(), QonversionError> {
    let launch_mode = match config.launch_mode {
        LaunchMode::SubscriptionManagement => enum_value(
            env,
            jni_str!("com/qonversion/android/sdk/dto/QLaunchMode"),
            jni_str!("SubscriptionManagement"),
            jni_sig!("Lcom/qonversion/android/sdk/dto/QLaunchMode;"),
        )?,
    };

    let environment = match config.environment {
        Environment::Sandbox => enum_value(
            env,
            jni_str!("com/qonversion/android/sdk/dto/QEnvironment"),
            jni_str!("Sandbox"),
            jni_sig!("Lcom/qonversion/android/sdk/dto/QEnvironment;"),
        )?,
        Environment::Production => enum_value(
            env,
            jni_str!("com/qonversion/android/sdk/dto/QEnvironment"),
            jni_str!("Production"),
            jni_sig!("Lcom/qonversion/android/sdk/dto/QEnvironment;"),
        )?,
    };

    let project_key = env
        .new_string(&config.project_key)
        .map_err(|e| QonversionError::Native {
            message: format!("failed to create project key string: {e}"),
        })?;

    let builder_class =
        find_class(env, jni_str!("com/qonversion/android/sdk/QonversionConfig$Builder"))?;
    let builder = env
        .new_object(
            &builder_class,
            jni_sig!(
                "(Landroid/content/Context;Ljava/lang/String;Lcom/qonversion/android/sdk/dto/QLaunchMode;)V"
            ),
            &[
                JValue::Object(context),
                JValue::Object(&project_key),
                JValue::Object(&launch_mode),
            ],
        )
        .map_err(|e| map_exception(env, e, "QonversionConfig.Builder"))?;

    let builder = env
        .call_method(
            &builder,
            jni_str!("setEnvironment"),
            jni_sig!(
                "(Lcom/qonversion/android/sdk/dto/QEnvironment;)Lcom/qonversion/android/sdk/QonversionConfig$Builder;"
            ),
            &[JValue::Object(&environment)],
        )
        .map_err(|e| map_exception(env, e, "setEnvironment"))?
        .l()
        .map_err(|e| QonversionError::Native {
            message: format!("setEnvironment returned unexpected type: {e}"),
        })?;

    let qonversion_config = env
        .call_method(
            &builder,
            jni_str!("build"),
            jni_sig!("()Lcom/qonversion/android/sdk/QonversionConfig;"),
            &[],
        )
        .map_err(|e| map_exception(env, e, "QonversionConfig.build"))?
        .l()
        .map_err(|e| QonversionError::Native {
            message: format!("build returned unexpected type: {e}"),
        })?;

    let qonversion_class = find_class(env, jni_str!("com/qonversion/android/sdk/Qonversion"))?;
    env.call_static_method(
        &qonversion_class,
        jni_str!("initialize"),
        jni_sig!(
            "(Lcom/qonversion/android/sdk/QonversionConfig;)Lcom/qonversion/android/sdk/Qonversion;"
        ),
        &[JValue::Object(&qonversion_config)],
    )
    .map_err(|e| map_exception(env, e, "Qonversion.initialize"))?;

    Ok(())
}

fn initialize_nocodes(
    env: &mut Env<'_>,
    context: &JObject<'_>,
    project_key: &str,
) -> Result<(), QonversionError> {
    let project_key = env
        .new_string(project_key)
        .map_err(|e| QonversionError::Native {
            message: format!("failed to create project key string: {e}"),
        })?;

    let builder_class = find_class(env, jni_str!("io/qonversion/nocodes/NoCodesConfig$Builder"))?;
    let builder = env
        .new_object(
            &builder_class,
            jni_sig!("(Landroid/content/Context;Ljava/lang/String;)V"),
            &[JValue::Object(context), JValue::Object(&project_key)],
        )
        .map_err(|e| map_exception(env, e, "NoCodesConfig.Builder"))?;

    let nocodes_config = env
        .call_method(
            &builder,
            jni_str!("build"),
            jni_sig!("()Lio/qonversion/nocodes/NoCodesConfig;"),
            &[],
        )
        .map_err(|e| map_exception(env, e, "NoCodesConfig.build"))?
        .l()
        .map_err(|e| QonversionError::Native {
            message: format!("NoCodesConfig.build returned unexpected type: {e}"),
        })?;

    let nocodes_class = find_class(env, jni_str!("io/qonversion/nocodes/NoCodes"))?;
    env.call_static_method(
        &nocodes_class,
        jni_str!("initialize"),
        jni_sig!("(Lio/qonversion/nocodes/NoCodesConfig;)Lio/qonversion/nocodes/NoCodes;"),
        &[JValue::Object(&nocodes_config)],
    )
    .map_err(|e| map_exception(env, e, "NoCodes.initialize"))?;

    Ok(())
}

fn find_class<'a>(
    env: &mut Env<'a>,
    name: &JNIStr,
) -> Result<jni::objects::JClass<'a>, QonversionError> {
    match env.find_class(name) {
        Ok(class) => Ok(class),
        Err(e) => {
            env.exception_clear();
            Err(QonversionError::HostMissing(format!(
                "JNI class `{}` not found ({e}). Add `implementation 'io.qonversion:no-codes:1.+'` to the Android app.",
                name.to_str()
            )))
        }
    }
}

fn enum_value<'a>(
    env: &mut Env<'a>,
    class_name: &JNIStr,
    field: &JNIStr,
    sig: impl AsRef<jni::signature::FieldSignature<'static>>,
) -> Result<JObject<'a>, QonversionError> {
    let class = find_class(env, class_name)?;
    env.get_static_field(&class, field, sig)
        .map_err(|e| {
            env.exception_clear();
            QonversionError::HostMissing(format!(
                "enum field `{}` not found ({e})",
                field.to_str()
            ))
        })?
        .l()
        .map_err(|e| QonversionError::Native {
            message: format!(
                "enum field `{}` was not an object: {e}",
                field.to_str()
            ),
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
