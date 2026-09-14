package io.dioxus.qonversion

import android.app.Activity
import android.content.Context
import android.os.Handler
import android.os.Looper
import com.qonversion.android.sdk.Qonversion
import com.qonversion.android.sdk.QonversionConfig
import com.qonversion.android.sdk.dto.QEnvironment
import com.qonversion.android.sdk.dto.QLaunchMode
import com.qonversion.android.sdk.dto.QUser
import com.qonversion.android.sdk.listeners.QonversionUserCallback
import io.qonversion.nocodes.NoCodes
import io.qonversion.nocodes.NoCodesConfig
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicReference

/**
 * Thin Kotlin host so Rust can call Qonversion + No-Codes via JNI.
 *
 * Bundled automatically by Dioxus CLI 0.7+ via manganis Android plugin metadata.
 * This Gradle library module already depends on `io.qonversion:no-codes`.
 */
object DioxusQonversionHost {
    /**
     * Initialize Qonversion (Subscription Management) and No-Codes.
     *
     * Hops to the main looper and **waits** — Rust `initialize` may run off the UI thread.
     *
     * @return `null` on success, or an error description on failure.
     */
    @JvmStatic
    fun initialize(context: Context, projectKey: String, sandbox: Boolean): String? {
        val trimmed = projectKey.trim()
        if (trimmed.isEmpty()) {
            return "project_key must not be empty"
        }
        return runOnMainSync {
            try {
                val environment = if (sandbox) QEnvironment.Sandbox else QEnvironment.Production
                val qonversionConfig = QonversionConfig.Builder(
                    context.applicationContext,
                    trimmed,
                    QLaunchMode.SubscriptionManagement,
                )
                    .setEnvironment(environment)
                    .build()
                Qonversion.initialize(qonversionConfig)

                val noCodesConfig = NoCodesConfig.Builder(context.applicationContext, trimmed).build()
                NoCodes.initialize(noCodesConfig)
                null
            } catch (t: Throwable) {
                t.message ?: t.toString()
            }
        }
    }

    /**
     * Present a No-Codes screen by context key (fire-and-present).
     *
     * Queues onto the Activity UI thread and returns immediately.
     *
     * @return `null` on success (queued), or an error description on failure.
     */
    @JvmStatic
    fun showScreen(activity: Activity, contextKey: String): String? {
        val trimmed = contextKey.trim()
        if (trimmed.isEmpty()) {
            return "context_key must not be empty"
        }
        val present = Runnable {
            try {
                NoCodes.shared.showScreen(trimmed)
            } catch (t: Throwable) {
                android.util.Log.e("DioxusQonversion", "showScreen failed: ${t.message}", t)
            }
        }
        if (Looper.myLooper() == Looper.getMainLooper()) {
            present.run()
        } else {
            activity.runOnUiThread(present)
        }
        return null
    }

    /**
     * Identify the Qonversion user with a stable app user id.
     *
     * Posts identify to the main looper and **waits** on the calling thread for
     * the SDK callback. Must not be invoked on the main thread (deadlock).
     * The Rust serial worker always calls this off-main.
     *
     * @return `null` on success, or an error description on failure.
     */
    @JvmStatic
    fun identify(userId: String): String? {
        val trimmed = userId.trim()
        if (trimmed.isEmpty()) {
            return "user_id must not be empty"
        }
        if (Looper.myLooper() == Looper.getMainLooper()) {
            return "identify must not be called on the Android main thread"
        }

        val latch = CountDownLatch(1)
        val errorRef = AtomicReference<String?>(null)
        Handler(Looper.getMainLooper()).post {
            try {
                Qonversion.shared.identify(trimmed, object : QonversionUserCallback {
                    override fun onSuccess(user: QUser) {
                        latch.countDown()
                    }

                    override fun onError(qError: com.qonversion.android.sdk.dto.QonversionError) {
                        errorRef.set(qError.description ?: qError.toString())
                        latch.countDown()
                    }
                })
            } catch (t: Throwable) {
                errorRef.set(t.message ?: t.toString())
                latch.countDown()
            }
        }
        latch.await()
        return errorRef.get()
    }

    /**
     * Clear the Qonversion user session.
     *
     * Hops to the main looper and **waits**.
     *
     * @return `null` on success, or an error description on failure.
     */
    @JvmStatic
    fun logout(): String? {
        return runOnMainSync {
            try {
                Qonversion.shared.logout()
                null
            } catch (t: Throwable) {
                t.message ?: t.toString()
            }
        }
    }

    private fun runOnMainSync(block: () -> String?): String? {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            return block()
        }
        val latch = CountDownLatch(1)
        val result = AtomicReference<String?>()
        Handler(Looper.getMainLooper()).post {
            result.set(block())
            latch.countDown()
        }
        latch.await()
        return result.get()
    }
}
