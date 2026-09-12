//! Android: call Qonversion + No-Codes via JNI.
//!
//! Requires the app to depend on `io.qonversion:no-codes:1.+` (pulls in the
//! Qonversion SDK). Context comes from `ndk_context` (initialized by Dioxus/wry).
//!
//! App classes are loaded via the Activity [`ClassLoader`] (plain `FindClass`
//! fails on non-UI threads). Qonversion/No-Codes calls are posted to the Java
//! main looper — No-Codes presents a separate Activity and must run on the UI
//! thread (mirrors iOS `DispatchQueue.main` in the Swift host).

use std::sync::mpsc;

use jni::objects::{JClass, JClassLoader, JObject, JString, JValue};
use jni::strings::JNIStr;
use jni::sys::{jlong, jobject};
use jni::{
    errors::ThrowRuntimeExAndDefault, jni_sig, jni_str, Env, EnvUnowned, JavaVM, NativeMethod,
};
use jni::objects::LoaderContext;

use crate::config::{Environment, InitConfig, LaunchMode};
use crate::error::QonversionError;

/// Minimal `Runnable` bytecode (`io.dioxus.qonversion.NativeRunnable`):
/// `public final class NativeRunnable implements Runnable { private final long ptr; ... native void run(); }`
///
/// Defined at runtime via [`Env::define_class`] so the app needs no Kotlin host.
const NATIVE_RUNNABLE_CLASS_BYTES: &[u8] = &[
    202, 254, 186, 190, 0, 0, 0, 52, 0, 21, 10, 0, 2, 0, 3, 7, 0, 4, 12, 0, 5, 0, 6, 1, 0, 16, 106,
    97, 118, 97, 47, 108, 97, 110, 103, 47, 79, 98, 106, 101, 99, 116, 1, 0, 6, 60, 105, 110, 105,
    116, 62, 1, 0, 3, 40, 41, 86, 9, 0, 8, 0, 9, 7, 0, 10, 12, 0, 11, 0, 12, 1, 0, 35, 105, 111,
    47, 100, 105, 111, 120, 117, 115, 47, 113, 111, 110, 118, 101, 114, 115, 105, 111, 110, 47, 78,
    97, 116, 105, 118, 101, 82, 117, 110, 110, 97, 98, 108, 101, 1, 0, 3, 112, 116, 114, 1, 0, 1,
    74, 7, 0, 14, 1, 0, 18, 106, 97, 118, 97, 47, 108, 97, 110, 103, 47, 82, 117, 110, 110, 97, 98,
    108, 101, 1, 0, 4, 40, 74, 41, 86, 1, 0, 4, 67, 111, 100, 101, 1, 0, 15, 76, 105, 110, 101, 78,
    117, 109, 98, 101, 114, 84, 97, 98, 108, 101, 1, 0, 3, 114, 117, 110, 1, 0, 10, 83, 111, 117,
    114, 99, 101, 70, 105, 108, 101, 1, 0, 19, 78, 97, 116, 105, 118, 101, 82, 117, 110, 110, 97,
    98, 108, 101, 46, 106, 97, 118, 97, 0, 49, 0, 8, 0, 2, 0, 1, 0, 13, 0, 1, 0, 18, 0, 11, 0, 12,
    0, 0, 0, 2, 0, 1, 0, 5, 0, 15, 0, 1, 0, 16, 0, 0, 0, 34, 0, 3, 0, 3, 0, 0, 0, 10, 42, 183, 0,
    1, 42, 31, 181, 0, 7, 177, 0, 0, 0, 1, 0, 17, 0, 0, 0, 6, 0, 1, 0, 0, 0, 6, 1, 1, 0, 18, 0, 6,
    0, 0, 0, 1, 0, 19, 0, 0, 0, 2, 0, 20,
];

type MainJob = Box<dyn FnOnce(&mut Env<'_>) -> Result<(), QonversionError> + Send>;

/// JNI global context pointer from `ndk_context` — safe to use from any attached thread.
#[derive(Clone, Copy)]
struct SendContextPtr(usize);

// Safety: the Activity Context global ref from ndk_context outlives these posts and is
// valid for JNI use on any attached thread (including the main looper).
unsafe impl Send for SendContextPtr {}

impl From<jni::errors::Error> for QonversionError {
    fn from(error: jni::errors::Error) -> Self {
        Self::Native {
            message: error.to_string(),
        }
    }
}

pub(crate) fn initialize(config: &InitConfig) -> Result<(), QonversionError> {
    let config = config.clone();
    run_on_main_looper_wait(move |env, context| {
        initialize_qonversion(env, context, &config)?;
        initialize_nocodes(env, context, &config.project_key)?;
        Ok(())
    })
}

pub(crate) fn show_screen(context_key: &str) -> Result<(), QonversionError> {
    let context_key = context_key.to_string();
    run_on_main_looper_async(move |env, context| {
        let key = env
            .new_string(&context_key)
            .map_err(|e| QonversionError::Native {
                message: format!("failed to create context key string: {e}"),
            })?;

        let nocodes_class = find_class(env, context, jni_str!("io/qonversion/nocodes/NoCodes"))?;
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

/// Run `f` on the Java main looper and wait for its result.
fn run_on_main_looper_wait<F>(f: F) -> Result<(), QonversionError>
where
    F: FnOnce(&mut Env<'_>, &JObject<'_>) -> Result<(), QonversionError> + Send + 'static,
{
    let (vm, context_raw) = java_vm_and_context()?;
    let (tx, rx) = mpsc::sync_channel(1);

    vm.attach_current_thread(|env| {
        let context = unsafe { JObject::from_raw(env, context_raw) };
        if is_main_looper(env)? {
            let _ = tx.send(f(env, &context));
            return Ok(());
        }

        let ctx = SendContextPtr(context_raw as usize);
        let job: MainJob = Box::new(move |env| {
            let context = unsafe { JObject::from_raw(env, ctx.0 as jobject) };
            let _ = tx.send(f(env, &context));
            Ok(())
        });
        post_runnable(env, &context, job)
    })?;

    rx.recv().map_err(|_| QonversionError::Native {
        message: "main-looper job channel closed before initialize completed".into(),
    })?
}

/// Post `f` to the Java main looper (fire-and-forget). Returns once queued.
fn run_on_main_looper_async<F>(f: F) -> Result<(), QonversionError>
where
    F: FnOnce(&mut Env<'_>, &JObject<'_>) -> Result<(), QonversionError> + Send + 'static,
{
    let (vm, context_raw) = java_vm_and_context()?;

    vm.attach_current_thread(|env| {
        let context = unsafe { JObject::from_raw(env, context_raw) };
        if is_main_looper(env)? {
            return f(env, &context);
        }

        let ctx = SendContextPtr(context_raw as usize);
        let job: MainJob = Box::new(move |env| {
            let context = unsafe { JObject::from_raw(env, ctx.0 as jobject) };
            if let Err(error) = f(env, &context) {
                eprintln!("dioxus-qonversion: main-looper job failed: {error}");
            }
            Ok(())
        });
        post_runnable(env, &context, job)
    })
}

fn java_vm_and_context() -> Result<(JavaVM, jobject), QonversionError> {
    let android_ctx = ndk_context::android_context();
    if android_ctx.context().is_null() || android_ctx.vm().is_null() {
        return Err(QonversionError::HostMissing(
            "ndk_context is not initialized (is this a Dioxus Android app?)".into(),
        ));
    }
    let vm = unsafe { JavaVM::from_raw(android_ctx.vm().cast()) };
    Ok((vm, android_ctx.context() as jobject))
}

fn is_main_looper(env: &mut Env<'_>) -> Result<bool, QonversionError> {
    let looper_class = env
        .find_class(jni_str!("android/os/Looper"))
        .map_err(|e| QonversionError::Native {
            message: format!("android.os.Looper not found: {e}"),
        })?;
    let main = env
        .call_static_method(
            &looper_class,
            jni_str!("getMainLooper"),
            jni_sig!("()Landroid/os/Looper;"),
            &[],
        )
        .map_err(|e| map_exception(env, e, "Looper.getMainLooper"))?
        .l()?;
    let mine = env
        .call_static_method(
            &looper_class,
            jni_str!("myLooper"),
            jni_sig!("()Landroid/os/Looper;"),
            &[],
        )
        .map_err(|e| map_exception(env, e, "Looper.myLooper"))?
        .l()?;
    if mine.is_null() {
        return Ok(false);
    }
    Ok(env.is_same_object(&main, &mine)?)
}

fn post_runnable(
    env: &mut Env<'_>,
    context: &JObject<'_>,
    job: MainJob,
) -> Result<(), QonversionError> {
    let runnable_class = ensure_native_runnable_class(env, context)?;
    let ptr = Box::into_raw(Box::new(job)) as jlong;
    let runnable = match env.new_object(
        &runnable_class,
        jni_sig!("(J)V"),
        &[JValue::Long(ptr)],
    ) {
        Ok(obj) => obj,
        Err(e) => {
            unsafe {
                drop(Box::from_raw(ptr as *mut MainJob));
            }
            return Err(map_exception(env, e, "NativeRunnable.<init>"));
        }
    };

    let looper_class = env
        .find_class(jni_str!("android/os/Looper"))
        .map_err(|e| QonversionError::Native {
            message: format!("android.os.Looper not found: {e}"),
        })?;
    let main_looper = env
        .call_static_method(
            &looper_class,
            jni_str!("getMainLooper"),
            jni_sig!("()Landroid/os/Looper;"),
            &[],
        )
        .map_err(|e| map_exception(env, e, "Looper.getMainLooper"))?
        .l()?;

    let handler_class = env
        .find_class(jni_str!("android/os/Handler"))
        .map_err(|e| QonversionError::Native {
            message: format!("android.os.Handler not found: {e}"),
        })?;
    let handler = env
        .new_object(
            &handler_class,
            jni_sig!("(Landroid/os/Looper;)V"),
            &[JValue::Object(&main_looper)],
        )
        .map_err(|e| map_exception(env, e, "Handler.<init>"))?;

    let posted = env
        .call_method(
            &handler,
            jni_str!("post"),
            jni_sig!("(Ljava/lang/Runnable;)Z"),
            &[JValue::Object(&runnable)],
        )
        .map_err(|e| map_exception(env, e, "Handler.post"))?
        .z()?;

    if !posted {
        unsafe {
            drop(Box::from_raw(ptr as *mut MainJob));
        }
        return Err(QonversionError::Native {
            message: "Handler.post returned false (main looper exiting?)".into(),
        });
    }
    Ok(())
}

fn ensure_native_runnable_class<'a>(
    env: &mut Env<'a>,
    context: &JObject<'_>,
) -> Result<JClass<'a>, QonversionError> {
    let loader = activity_class_loader(env, context)?;
    match LoaderContext::Loader(&loader).load_class(
        env,
        jni_str!("io.dioxus.qonversion.NativeRunnable"),
        false,
    ) {
        Ok(class) => Ok(class),
        Err(_) => {
            env.exception_clear();
            let class = env
                .define_class(
                    Some(jni_str!("io/dioxus/qonversion/NativeRunnable")),
                    &loader,
                    NATIVE_RUNNABLE_CLASS_BYTES,
                )
                .map_err(|e| QonversionError::Native {
                    message: format!("failed to define NativeRunnable: {e}"),
                })?;
            // Safety: `native_runnable_run` matches `()V` instance method ABI.
            unsafe {
                env.register_native_methods(
                    &class,
                    &[NativeMethod::from_raw_parts(
                        jni_str!("run"),
                        jni_str!("()V"),
                        native_runnable_run as *mut std::ffi::c_void,
                    )],
                )
                .map_err(|e| QonversionError::Native {
                    message: format!("RegisterNatives(NativeRunnable.run) failed: {e}"),
                })?;
            }
            Ok(class)
        }
    }
}

extern "system" fn native_runnable_run<'local>(
    mut unowned_env: EnvUnowned<'local>,
    this: JObject<'local>,
) {
    unowned_env
        .with_env(|env| -> Result<(), jni::errors::Error> {
            let ptr = env
                .get_field(&this, jni_str!("ptr"), jni_sig!("J"))?
                .j()?;
            if ptr == 0 {
                return Ok(());
            }
            // Prevent double-free if run() is invoked twice.
            env.set_field(&this, jni_str!("ptr"), jni_sig!("J"), JValue::Long(0))?;
            let job = unsafe { Box::from_raw(ptr as *mut MainJob) };
            let _ = (*job)(env);
            Ok(())
        })
        .resolve::<ThrowRuntimeExAndDefault>();
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

fn initialize_qonversion(
    env: &mut Env<'_>,
    context: &JObject<'_>,
    config: &InitConfig,
) -> Result<(), QonversionError> {
    let launch_mode = match config.launch_mode {
        LaunchMode::SubscriptionManagement => enum_value(
            env,
            context,
            jni_str!("com/qonversion/android/sdk/dto/QLaunchMode"),
            jni_str!("SubscriptionManagement"),
            jni_sig!("Lcom/qonversion/android/sdk/dto/QLaunchMode;"),
        )?,
    };

    let environment = match config.environment {
        Environment::Sandbox => enum_value(
            env,
            context,
            jni_str!("com/qonversion/android/sdk/dto/QEnvironment"),
            jni_str!("Sandbox"),
            jni_sig!("Lcom/qonversion/android/sdk/dto/QEnvironment;"),
        )?,
        Environment::Production => enum_value(
            env,
            context,
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

    let builder_class = find_class(
        env,
        context,
        jni_str!("com/qonversion/android/sdk/QonversionConfig$Builder"),
    )?;
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

    let qonversion_class =
        find_class(env, context, jni_str!("com/qonversion/android/sdk/Qonversion"))?;
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

    let builder_class =
        find_class(env, context, jni_str!("io/qonversion/nocodes/NoCodesConfig$Builder"))?;
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

    let nocodes_class = find_class(env, context, jni_str!("io/qonversion/nocodes/NoCodes"))?;
    env.call_static_method(
        &nocodes_class,
        jni_str!("initialize"),
        jni_sig!("(Lio/qonversion/nocodes/NoCodesConfig;)Lio/qonversion/nocodes/NoCodes;"),
        &[JValue::Object(&nocodes_config)],
    )
    .map_err(|e| map_exception(env, e, "NoCodes.initialize"))?;

    Ok(())
}

/// Load an app/SDK class. Tries `FindClass`, then the Activity [`ClassLoader`].
fn find_class<'a>(
    env: &mut Env<'a>,
    context: &JObject<'_>,
    name: &JNIStr,
) -> Result<JClass<'a>, QonversionError> {
    match env.find_class(name) {
        Ok(class) => Ok(class),
        Err(_) => {
            env.exception_clear();
            let dotted = name.to_str().replace('/', ".");
            let loader = activity_class_loader(env, context)?;
            let class_name = env.new_string(&dotted).map_err(|e| QonversionError::Native {
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
                        "JNI class `{}` not found ({e}). Add `implementation 'io.qonversion:no-codes:1.+'` to the Android app.",
                        name.to_str()
                    ))
                })?
                .l()
                .map_err(|e| QonversionError::Native {
                    message: format!("loadClass returned unexpected type: {e}"),
                })?;
            env.cast_local::<JClass>(class).map_err(|e| {
                QonversionError::Native {
                    message: format!("failed to cast loaded class: {e}"),
                }
            })
        }
    }
}

fn enum_value<'a>(
    env: &mut Env<'a>,
    context: &JObject<'_>,
    class_name: &JNIStr,
    field: &JNIStr,
    sig: impl AsRef<jni::signature::FieldSignature<'static>>,
) -> Result<JObject<'a>, QonversionError> {
    let class = find_class(env, context, class_name)?;
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
