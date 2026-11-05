//! Test-only JNI bootstrap. No Activity is created; the application context is
//! retained until process exit because the SDK queue/callbacks can outlive a call.
use jni::sys::{jint, jobject, jstring, JNIEnv, JavaVM, JNI_OK};
use std::sync::OnceLock;

static CONTEXT: OnceLock<Result<(), String>> = OnceLock::new();

unsafe fn prepare(env: *mut JNIEnv, context: jobject) -> Result<(), String> {
    CONTEXT
        .get_or_init(|| {
            if context.is_null() {
                return Err("bootstrap: missing Android application context".into());
            }
            let mut vm: *mut JavaVM = std::ptr::null_mut();
            if ((**env).v1_6.GetJavaVM)(env, &mut vm) != JNI_OK || vm.is_null() {
                return Err("bootstrap: GetJavaVM failed".into());
            }
            let global = ((**env).v1_6.NewGlobalRef)(env, context);
            if global.is_null() {
                return Err("bootstrap: NewGlobalRef failed".into());
            }
            // Exactly once, before any SDK operation. Deliberately keep this global
            // reference for process lifetime; never release it under a live worker.
            ndk_context::initialize_android_context(vm.cast(), global.cast());
            Ok(())
        })
        .clone()
}

/// Called only by AndroidJUnitRunner's instrumentation thread. A non-null
/// string means failure; null means success (or a pending JVM exception).
#[no_mangle]
pub unsafe extern "system" fn Java_io_dioxus_qonversion_nativetest_RustBridge_runNative(
    env: *mut JNIEnv,
    _receiver: jobject,
    context: jobject,
) -> jstring {
    let result = super::boundary(|| {
        prepare(env, context)?;
        super::initialize()?;
        super::query()
    });
    match result {
        Ok(()) => std::ptr::null_mut(),
        Err(message) => {
            // Preserve a pending JVM error (e.g. NewGlobalRef OOM); JNI must
            // return immediately rather than allocate another string under it.
            if ((**env).v1_6.ExceptionCheck)(env) {
                return std::ptr::null_mut();
            }
            let utf16: Vec<u16> = message.encode_utf16().collect();
            ((**env).v1_6.NewString)(env, utf16.as_ptr(), utf16.len() as jint)
        }
    }
}
