package io.dioxus.qonversion.nativetest

// The target application's ClassLoader owns the JNI library. This object is
// first accessed only after the instrumentation test checks its opt-in flag.
object RustBridge {
    init { System.loadLibrary("native_remote_config_test") }
    external fun runNative(context: android.content.Context): String?
}
