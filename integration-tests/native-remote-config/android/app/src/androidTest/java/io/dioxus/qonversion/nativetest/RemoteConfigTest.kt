package io.dioxus.qonversion.nativetest

import android.content.Intent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertNull
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class RemoteConfigTest {
    @Test(timeout = 60000)
    fun realRemoteConfig() {
        assumeTrue("Native SDK test disabled: pass -PrunNativeRemoteConfig=true",
            InstrumentationRegistry.getArguments().getString("runNativeRemoteConfig") == "true")
        // Instrumentation/JUnit worker is not the main looper. The production
        // Kotlin host posts SDK initialization and requests to the main looper.
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext.applicationContext
        // Qonversion defers its initial launch until the app enters the foreground.
        val activity = instrumentation.startActivitySync(
            Intent(context, ForegroundActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        )
        try {
            assertNull(RustBridge.runNative(context))
        } finally {
            instrumentation.runOnMainSync { activity.finish() }
        }
    }
}
