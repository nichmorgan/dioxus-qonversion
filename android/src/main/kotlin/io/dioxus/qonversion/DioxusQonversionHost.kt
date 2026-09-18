package io.dioxus.qonversion

import android.app.Activity
import android.content.Context
import android.content.pm.PackageManager
import android.os.Handler
import android.os.Looper
import com.android.billingclient.api.BillingClient
import com.android.billingclient.api.BillingClientStateListener
import com.android.billingclient.api.BillingResult
import com.android.billingclient.api.PendingPurchasesParams
import com.qonversion.android.sdk.Qonversion
import com.qonversion.android.sdk.QonversionConfig
import com.qonversion.android.sdk.dto.QEnvironment
import com.qonversion.android.sdk.dto.QLaunchMode
import com.qonversion.android.sdk.dto.QRemoteConfig
import com.qonversion.android.sdk.dto.QRemoteConfigurationSource
import com.qonversion.android.sdk.dto.QUser
import com.qonversion.android.sdk.dto.QonversionErrorCode
import com.qonversion.android.sdk.dto.experiments.QExperiment
import com.qonversion.android.sdk.listeners.QonversionRemoteConfigCallback
import com.qonversion.android.sdk.listeners.QonversionUserCallback
import io.qonversion.nocodes.NoCodes
import io.qonversion.nocodes.NoCodesConfig
import io.qonversion.nocodes.dto.QAction
import io.qonversion.nocodes.dto.QNoCodeScreen
import io.qonversion.nocodes.error.ErrorCode
import io.qonversion.nocodes.error.NoCodesError
import io.qonversion.nocodes.interfaces.NoCodesDelegate
import io.qonversion.nocodes.interfaces.NoCodesScreenLoadCallback
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Thin Kotlin host so Rust can call Qonversion + No-Codes via JNI.
 *
 * Bundled automatically by Dioxus CLI 0.7+ via manganis Android plugin metadata.
 * This Gradle library module already depends on `io.qonversion:no-codes`.
 */
object DioxusQonversionHost {
    /**
     * Returned when [ensureStoreAvailable] is invoked on the main looper.
     * Rust treats this as skip-preflight (not store-unavailable) so UI-thread
     * `show_screen` still presents.
     */
    private const val SKIP_PREFLIGHT_MAIN_THREAD: String = "SKIP_PREFLIGHT_MAIN_THREAD"

    /** Must match Rust `helpers::HOST_TIMEOUT_SENTINEL`. */
    private const val HOST_TIMEOUT_SENTINEL: String = "dioxus_qonversion:timeout"

    private const val PLAY_STORE_PACKAGE = "com.android.vending"
    private const val STORE_PREFLIGHT_TIMEOUT_MS = 4000L
    /** Matches Rust `DEFAULT_SDK_TIMEOUT` for the initialize main hop. */
    private const val INIT_TIMEOUT_MS = 8000L

    private val screenFailedDelegate = object : NoCodesDelegate {
        override fun onActionFinishedExecuting(action: QAction) {
            notifyNativeSafe("notifyScreenEvent") {
                notifyScreenEvent(encodeScreenEvent("action_finished", action = action.type.type))
            }
        }

        override fun onActionFailedToExecute(action: QAction) {
            val message = action.error?.toString() ?: "No-Codes action failed"
            notifyNativeSafe("notifyScreenEvent") {
                notifyScreenEvent(
                    encodeScreenEvent("action_failed", action = action.type.type, message = message)
                )
            }
        }

        override fun onFinished() {
            notifyNativeSafe("notifyScreenEvent") {
                notifyScreenEvent(encodeScreenEvent("finished"))
            }
        }

        override fun onCustomAction(value: String) {
            notifyNativeSafe("notifyScreenEvent") {
                notifyScreenEvent(encodeScreenEvent("custom_action", value = value))
            }
        }

        override fun onScreenFailedToLoad(error: NoCodesError) {
            val storeUnavailable = isStoreUnavailable(error)
            notifyNativeSafe("notifyScreenFailed") {
                notifyScreenFailed(storeUnavailable, error.toString())
            }
            try {
                NoCodes.shared.close()
            } catch (t: Throwable) {
                android.util.Log.e("DioxusQonversion", "NoCodes.close failed: ${t.message}", t)
            }
        }
    }

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
        return runOnMainSync(INIT_TIMEOUT_MS) {
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

                val noCodesConfig = NoCodesConfig.Builder(context.applicationContext, trimmed)
                    .setDelegate(screenFailedDelegate)
                    .build()
                NoCodes.initialize(noCodesConfig)
                null
            } catch (t: Throwable) {
                t.message ?: t.toString()
            }
        }
    }

    /**
     * Probe Play Billing before presenting a No-Codes screen.
     *
     * @return `null` if BillingClient connected with `OK`;
     * [SKIP_PREFLIGHT_MAIN_THREAD] if called on the main looper (must not latch);
     * any other string if the store is unavailable.
     */
    @JvmStatic
    fun ensureStoreAvailable(context: Context): String? {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            return SKIP_PREFLIGHT_MAIN_THREAD
        }
        if (!isPlayStoreInstalled(context)) {
            return "Play Store is not installed"
        }

        val latch = CountDownLatch(1)
        val result = AtomicReference<String?>(null)
        val finished = AtomicBoolean(false)
        val clientRef = AtomicReference<BillingClient?>(null)
        val appContext = context.applicationContext

        Handler(Looper.getMainLooper()).post {
            if (finished.get()) {
                return@post
            }
            try {
                val billingClient = BillingClient.newBuilder(appContext)
                    .setListener { _, _ -> }
                    .enablePendingPurchases(
                        PendingPurchasesParams.newBuilder().enableOneTimeProducts().build(),
                    )
                    .build()
                if (finished.get()) {
                    endBillingClient(billingClient)
                    return@post
                }
                clientRef.set(billingClient)
                if (finished.get()) {
                    endBillingClient(clientRef.getAndSet(null))
                    return@post
                }
                billingClient.startConnection(object : BillingClientStateListener {
                    override fun onBillingSetupFinished(billingResult: BillingResult) {
                        val code = billingResult.responseCode
                        val unavailable = when (code) {
                            BillingClient.BillingResponseCode.OK -> null
                            else -> {
                                val debug = billingResult.debugMessage ?: ""
                                "billing unavailable: $code $debug"
                            }
                        }
                        completeStoreProbe(clientRef, result, finished, latch, unavailable)
                    }

                    override fun onBillingServiceDisconnected() {
                        completeStoreProbe(
                            clientRef,
                            result,
                            finished,
                            latch,
                            "billing unavailable: SERVICE_DISCONNECTED",
                        )
                    }
                })
            } catch (t: Throwable) {
                completeStoreProbe(clientRef, result, finished, latch, t.message ?: t.toString())
            }
        }

        if (!latch.await(STORE_PREFLIGHT_TIMEOUT_MS, TimeUnit.MILLISECONDS)) {
            completeStoreProbe(
                clientRef,
                result,
                finished,
                latch,
                "billing connection timed out",
            )
        }
        return result.get()
    }

    /**
     * JNI target registered from Rust during initialize.
     * `storeUnavailable` is true for Play Billing / SERVICE_DISCONNECTED failures.
     */
    @JvmStatic
    external fun notifyScreenFailed(storeUnavailable: Boolean, message: String)

    /**
     * JNI target registered from Rust during initialize.
     * [json] is a screen-event envelope (`kind` + optional `action` / `message` / `value`).
     */
    @JvmStatic
    external fun notifyScreenEvent(json: String)

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
     * Load a No-Codes screen by context key without presenting it (ask-first).
     *
     * Posts to the main looper and **waits** on the calling thread for the SDK
     * callback. Must not be invoked on the main thread (deadlock).
     *
     * @return JSON envelope (`ok:true` + id/context_key, or `ok:false` + error).
     */
    @JvmStatic
    fun loadScreen(contextKey: String, timeoutMs: Long): String {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            return encodeLoadScreenError("load_screen must not be called on the Android main thread", false)
        }
        val trimmed = contextKey.trim()
        if (trimmed.isEmpty()) {
            return encodeLoadScreenError("context_key must not be empty", false)
        }

        return awaitOnMain(
            timeoutMs,
            encodeLoadScreenError("load screen timed out", false, timedOut = true),
        ) { complete ->
            try {
                NoCodes.shared.loadScreen(trimmed, object : NoCodesScreenLoadCallback {
                    override fun onSuccess(screen: QNoCodeScreen) {
                        complete(encodeLoadScreenSuccess(screen))
                    }

                    override fun onError(error: NoCodesError) {
                        complete(
                            encodeLoadScreenError(
                                error.toString(),
                                error.code == ErrorCode.ScreenNotFound,
                            ),
                        )
                    }
                })
            } catch (t: Throwable) {
                complete(encodeLoadScreenError(t.message ?: t.toString(), false))
            }
        }
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
    fun identify(userId: String, timeoutMs: Long): String? {
        val trimmed = userId.trim()
        if (trimmed.isEmpty()) {
            return "user_id must not be empty"
        }
        if (Looper.myLooper() == Looper.getMainLooper()) {
            return "identify must not be called on the Android main thread"
        }

        return awaitOnMain(timeoutMs, HOST_TIMEOUT_SENTINEL) { complete ->
            try {
                Qonversion.shared.identify(trimmed, object : QonversionUserCallback {
                    override fun onSuccess(user: QUser) {
                        complete(null)
                    }

                    override fun onError(qError: com.qonversion.android.sdk.dto.QonversionError) {
                        complete(qError.description ?: qError.toString())
                    }
                })
            } catch (t: Throwable) {
                complete(t.message ?: t.toString())
            }
        }
    }

    /**
     * Fetch Remote Config for [contextKey], or the empty context key when [contextKey] is null/blank.
     *
     * Posts to the main looper and **waits** on the calling thread for the SDK callback.
     * Must not be invoked on the main thread (deadlock). The Rust serial worker always
     * calls this off-main.
     *
     * @return JSON envelope string (`ok:true` + payload, or `ok:false` + error). Never null.
     */
    @JvmStatic
    fun remoteConfig(contextKey: String?, timeoutMs: Long): String {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            return encodeRemoteConfigError("remote_config must not be called on the Android main thread")
        }

        val key = contextKey?.trim()?.takeIf { it.isNotEmpty() }
        return awaitOnMain(
            timeoutMs,
            encodeRemoteConfigError("remote config timed out", timedOut = true),
        ) { complete ->
            try {
                val callback = object : QonversionRemoteConfigCallback {
                    override fun onSuccess(remoteConfig: QRemoteConfig) {
                        complete(encodeRemoteConfigSuccess(remoteConfig))
                    }

                    override fun onError(qError: com.qonversion.android.sdk.dto.QonversionError) {
                        complete(encodeRemoteConfigError(qError.description ?: qError.toString()))
                    }
                }
                if (key == null) {
                    Qonversion.shared.remoteConfig(callback)
                } else {
                    Qonversion.shared.remoteConfig(key, callback)
                }
            } catch (t: Throwable) {
                complete(encodeRemoteConfigError(t.message ?: t.toString()))
            }
        }
    }

    /**
     * Clear the Qonversion user session.
     *
     * Hops to the main looper and **waits**.
     *
     * @return `null` on success, or an error description on failure.
     */
    @JvmStatic
    fun logout(timeoutMs: Long): String? {
        return runOnMainSync(timeoutMs) {
            try {
                Qonversion.shared.logout()
                null
            } catch (t: Throwable) {
                t.message ?: t.toString()
            }
        }
    }

    private fun completeStoreProbe(
        clientRef: AtomicReference<BillingClient?>,
        result: AtomicReference<String?>,
        finished: AtomicBoolean,
        latch: CountDownLatch,
        unavailable: String?,
    ) {
        if (!finished.compareAndSet(false, true)) {
            return
        }
        result.set(unavailable)
        endBillingClient(clientRef.get())
        latch.countDown()
    }

    private fun endBillingClient(client: BillingClient?) {
        if (client == null) {
            return
        }
        try {
            client.endConnection()
        } catch (t: Throwable) {
            android.util.Log.e("DioxusQonversion", "BillingClient.endConnection failed: ${t.message}", t)
        }
    }

    private fun isPlayStoreInstalled(context: Context): Boolean {
        return try {
            context.packageManager.getPackageInfo(PLAY_STORE_PACKAGE, 0)
            true
        } catch (_: PackageManager.NameNotFoundException) {
            false
        }
    }

    private fun isStoreUnavailable(error: NoCodesError): Boolean {
        val qError = error.qonversionError
        if (qError != null) {
            when (qError.code) {
                QonversionErrorCode.PlayStoreError,
                QonversionErrorCode.BillingUnavailable,
                -> return true
                else -> Unit
            }
            if (looksLikeStoreFailure("${qError.additionalMessage} ${qError.description}")) {
                return true
            }
        }
        return looksLikeStoreFailure(
            "${error.code} ${error.details ?: ""} ${error.cause ?: ""} $error",
        )
    }

    private fun looksLikeStoreFailure(text: String): Boolean {
        val upper = text.uppercase()
        return upper.contains("PLAYSTOREERROR") ||
            upper.contains("BILLINGUNAVAILABLE") ||
            upper.contains("BILLING_UNAVAILABLE") ||
            upper.contains("SERVICE_DISCONNECTED") ||
            upper.contains("IN-APP BILLING API VERSION")
    }

    private fun runOnMainSync(timeoutMs: Long, block: () -> String?): String? {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            return block()
        }
        return awaitOnMain(timeoutMs, HOST_TIMEOUT_SENTINEL) { complete ->
            complete(block())
        }
    }

    private fun <T> awaitOnMain(timeoutMs: Long, timeoutValue: T, work: (complete: (T) -> Unit) -> Unit): T {
        val latch = CountDownLatch(1)
        val finished = AtomicBoolean(false)
        val result = AtomicReference(timeoutValue)
        val complete: (T) -> Unit = { value ->
            if (finished.compareAndSet(false, true)) {
                result.set(value)
                latch.countDown()
            }
        }
        Handler(Looper.getMainLooper()).post {
            if (finished.get()) {
                return@post
            }
            try {
                work(complete)
            } catch (t: Throwable) {
                android.util.Log.e("DioxusQonversion", "awaitOnMain work failed: ${t.message}", t)
                complete(timeoutValue)
            }
        }
        if (!latch.await(timeoutMs.coerceAtLeast(0L), TimeUnit.MILLISECONDS)) {
            complete(timeoutValue)
        }
        return result.get()
    }

    private fun encodeRemoteConfigSuccess(config: QRemoteConfig): String {
        return try {
            val root = JSONObject()
            root.put("ok", true)
            root.put("payload", jsonValue(config.payload))
            root.put("source", encodeSource(config.source))
            val experiment = config.experiment
            if (experiment == null) {
                root.put("experiment", JSONObject.NULL)
            } else {
                root.put("experiment", encodeExperiment(experiment))
            }
            root.toString()
        } catch (t: Throwable) {
            encodeRemoteConfigError(t.message ?: t.toString())
        }
    }

    private fun notifyNativeSafe(name: String, block: () -> Unit) {
        try {
            block()
        } catch (t: Throwable) {
            android.util.Log.e("DioxusQonversion", "$name failed: ${t.message}", t)
        }
    }

    private fun encodeScreenEvent(
        kind: String,
        action: String? = null,
        message: String? = null,
        value: String? = null,
    ): String {
        val root = JSONObject()
        root.put("kind", kind)
        if (action != null) {
            root.put("action", action)
        }
        if (message != null) {
            root.put("message", message)
        }
        if (value != null) {
            root.put("value", value)
        }
        return root.toString()
    }

    private fun encodeLoadScreenSuccess(screen: QNoCodeScreen): String {
        return try {
            val root = JSONObject()
            root.put("ok", true)
            root.put("id", screen.id)
            root.put("context_key", screen.contextKey)
            root.toString()
        } catch (t: Throwable) {
            encodeLoadScreenError(t.message ?: t.toString(), false)
        }
    }

    private fun encodeLoadScreenError(
        message: String,
        screenNotFound: Boolean,
        timedOut: Boolean = false,
    ): String {
        val root = JSONObject()
        root.put("ok", false)
        root.put("error", message)
        root.put("screen_not_found", screenNotFound)
        if (timedOut) {
            root.put("timed_out", true)
        }
        return root.toString()
    }

    private fun encodeRemoteConfigError(message: String, timedOut: Boolean = false): String {
        val root = JSONObject()
        root.put("ok", false)
        root.put("error", message)
        if (timedOut) {
            root.put("timed_out", true)
        }
        return root.toString()
    }

    private fun encodeSource(source: QRemoteConfigurationSource): JSONObject {
        val obj = JSONObject()
        obj.put("id", source.id)
        obj.put("name", source.name)
        obj.put("assignment_type", source.assignmentType.type)
        obj.put("type", source.type.type)
        val contextKey = source.contextKey
        if (contextKey.isNullOrEmpty()) {
            obj.put("context_key", JSONObject.NULL)
        } else {
            obj.put("context_key", contextKey)
        }
        return obj
    }

    private fun encodeExperiment(experiment: QExperiment): JSONObject {
        val group = JSONObject()
        group.put("id", experiment.group.id)
        group.put("name", experiment.group.name)
        group.put("type", experiment.group.type.type)
        val obj = JSONObject()
        obj.put("id", experiment.id)
        obj.put("name", experiment.name)
        obj.put("group", group)
        return obj
    }

    private fun jsonValue(value: Any?): Any {
        return when (value) {
            null -> JSONObject.NULL
            JSONObject.NULL -> JSONObject.NULL
            is JSONObject, is JSONArray -> value
            is Map<*, *> -> {
                val obj = JSONObject()
                for ((k, v) in value) {
                    if (k is String) {
                        obj.put(k, jsonValue(v))
                    }
                }
                obj
            }
            is Collection<*> -> {
                val arr = JSONArray()
                for (item in value) {
                    arr.put(jsonValue(item))
                }
                arr
            }
            is Array<*> -> {
                val arr = JSONArray()
                for (item in value) {
                    arr.put(jsonValue(item))
                }
                arr
            }
            is Boolean, is Number, is String -> value
            else -> value.toString()
        }
    }
}
