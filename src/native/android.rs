//! Android: call Qonversion + No-Codes via JNI.
//!
//! Requires the app to depend on `io.qonversion:no-codes:1.+` (pulls in the
//! Qonversion SDK). Context comes from `ndk_context` (initialized by Dioxus/wry).

use jni::objects::{JObject, JValue};
use jni::JavaVM;

use crate::config::{Environment, InitConfig, LaunchMode};
use crate::error::QonversionError;

pub(crate) fn initialize(config: &InitConfig) -> Result<(), QonversionError> {
    let android_ctx = ndk_context::android_context();
    if android_ctx.context().is_null() || android_ctx.vm().is_null() {
        return Err(QonversionError::HostMissing(
            "ndk_context is not initialized (is this a Dioxus Android app?)".into(),
        ));
    }

    let vm = unsafe { JavaVM::from_raw(android_ctx.vm().cast()) }.map_err(|e| {
        QonversionError::Native {
            message: format!("failed to attach JavaVM: {e}"),
        }
    })?;

    let mut env = vm.attach_current_thread().map_err(|e| QonversionError::Native {
        message: format!("failed to attach JNI thread: {e}"),
    })?;

    let context = unsafe { JObject::from_raw(android_ctx.context() as jni::sys::jobject) };

    initialize_qonversion(&mut env, &context, config)?;
    initialize_nocodes(&mut env, &context, &config.project_key)?;

    Ok(())
}

fn initialize_qonversion(
    env: &mut jni::AttachGuard<'_>,
    context: &JObject<'_>,
    config: &InitConfig,
) -> Result<(), QonversionError> {
    let launch_mode = match config.launch_mode {
        LaunchMode::SubscriptionManagement => enum_value(
            env,
            "com/qonversion/android/sdk/dto/QLaunchMode",
            "SubscriptionManagement",
        )?,
    };

    let environment = match config.environment {
        Environment::Sandbox => {
            enum_value(env, "com/qonversion/android/sdk/dto/QEnvironment", "Sandbox")?
        }
        Environment::Production => {
            enum_value(env, "com/qonversion/android/sdk/dto/QEnvironment", "Production")?
        }
    };

    let project_key = env
        .new_string(&config.project_key)
        .map_err(|e| QonversionError::Native {
            message: format!("failed to create project key string: {e}"),
        })?;

    let builder_class = find_class(env, "com/qonversion/android/sdk/QonversionConfig$Builder")?;
    let builder = env
        .new_object(
            &builder_class,
            "(Landroid/content/Context;Ljava/lang/String;Lcom/qonversion/android/sdk/dto/QLaunchMode;)V",
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
            "setEnvironment",
            "(Lcom/qonversion/android/sdk/dto/QEnvironment;)Lcom/qonversion/android/sdk/QonversionConfig$Builder;",
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
            "build",
            "()Lcom/qonversion/android/sdk/QonversionConfig;",
            &[],
        )
        .map_err(|e| map_exception(env, e, "QonversionConfig.build"))?
        .l()
        .map_err(|e| QonversionError::Native {
            message: format!("build returned unexpected type: {e}"),
        })?;

    let qonversion_class = find_class(env, "com/qonversion/android/sdk/Qonversion")?;
    env.call_static_method(
        &qonversion_class,
        "initialize",
        "(Lcom/qonversion/android/sdk/QonversionConfig;)Lcom/qonversion/android/sdk/Qonversion;",
        &[JValue::Object(&qonversion_config)],
    )
    .map_err(|e| map_exception(env, e, "Qonversion.initialize"))?;

    Ok(())
}

fn initialize_nocodes(
    env: &mut jni::AttachGuard<'_>,
    context: &JObject<'_>,
    project_key: &str,
) -> Result<(), QonversionError> {
    let project_key = env
        .new_string(project_key)
        .map_err(|e| QonversionError::Native {
            message: format!("failed to create project key string: {e}"),
        })?;

    let builder_class = find_class(env, "io/qonversion/nocodes/NoCodesConfig$Builder")?;
    let builder = env
        .new_object(
            &builder_class,
            "(Landroid/content/Context;Ljava/lang/String;)V",
            &[JValue::Object(context), JValue::Object(&project_key)],
        )
        .map_err(|e| map_exception(env, e, "NoCodesConfig.Builder"))?;

    let nocodes_config = env
        .call_method(
            &builder,
            "build",
            "()Lio/qonversion/nocodes/NoCodesConfig;",
            &[],
        )
        .map_err(|e| map_exception(env, e, "NoCodesConfig.build"))?
        .l()
        .map_err(|e| QonversionError::Native {
            message: format!("NoCodesConfig.build returned unexpected type: {e}"),
        })?;

    let nocodes_class = find_class(env, "io/qonversion/nocodes/NoCodes")?;
    env.call_static_method(
        &nocodes_class,
        "initialize",
        "(Lio/qonversion/nocodes/NoCodesConfig;)Lio/qonversion/nocodes/NoCodes;",
        &[JValue::Object(&nocodes_config)],
    )
    .map_err(|e| map_exception(env, e, "NoCodes.initialize"))?;

    Ok(())
}

fn find_class<'a>(
    env: &mut jni::AttachGuard<'a>,
    name: &str,
) -> Result<jni::objects::JClass<'a>, QonversionError> {
    match env.find_class(name) {
        Ok(class) => Ok(class),
        Err(e) => {
            let _ = env.exception_clear();
            Err(QonversionError::HostMissing(format!(
                "JNI class `{name}` not found ({e}). Add `implementation 'io.qonversion:no-codes:1.+'` to the Android app."
            )))
        }
    }
}

fn enum_value<'a>(
    env: &mut jni::AttachGuard<'a>,
    class_name: &str,
    field: &str,
) -> Result<JObject<'a>, QonversionError> {
    let class = find_class(env, class_name)?;
    let sig = format!("L{class_name};");
    env.get_static_field(&class, field, &sig)
        .map_err(|e| {
            let _ = env.exception_clear();
            QonversionError::HostMissing(format!(
                "enum field `{class_name}.{field}` not found ({e})"
            ))
        })?
        .l()
        .map_err(|e| QonversionError::Native {
            message: format!("enum field `{class_name}.{field}` was not an object: {e}"),
        })
}

fn map_exception(
    env: &mut jni::AttachGuard<'_>,
    err: jni::errors::Error,
    what: &str,
) -> QonversionError {
    if let Ok(true) = env.exception_check() {
        let message = describe_exception(env).unwrap_or_else(|| err.to_string());
        let _ = env.exception_clear();
        return QonversionError::Native {
            message: format!("{what}: {message}"),
        };
    }
    QonversionError::Native {
        message: format!("{what}: {err}"),
    }
}

fn describe_exception(env: &mut jni::AttachGuard<'_>) -> Option<String> {
    let throwable = env.exception_occurred().ok()?;
    let _ = env.exception_clear();
    let message = env
        .call_method(throwable, "toString", "()Ljava/lang/String;", &[])
        .ok()?
        .l()
        .ok()?;
    let jstr = env.get_string((&message).into()).ok()?;
    Some(jstr.into())
}
