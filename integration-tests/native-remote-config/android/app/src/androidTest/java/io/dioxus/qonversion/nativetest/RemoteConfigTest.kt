package io.dioxus.qonversion.nativetest

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
        assumeTrue("Native SDK test disabled: use run.py android --run",
            InstrumentationRegistry.getArguments().getString("runNativeRemoteConfig") == "true")
        // Instrumentation/JUnit worker is not the main looper. The production
        // Kotlin host posts SDK initialization and requests to the main looper.
        val context = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
        assertNull(RustBridge.runNative(context))
    }
}
