import Foundation
import Qonversion
import NoCodes

@_silgen_name("dioxus_qonversion_notify_screen_failed")
func dioxus_qonversion_notify_screen_failed(_ storeUnavailable: Int32, _ message: UnsafePointer<CChar>?)

@_silgen_name("dioxus_qonversion_notify_screen_event")
func dioxus_qonversion_notify_screen_event(_ json: UnsafePointer<CChar>?)

/// Thin ObjC-visible host so Rust can call Qonversion + No-Codes via `objc`.
///
/// Compile this file into your Dioxus iOS target and add the Qonversion iOS SDK
/// (SPM: https://github.com/qonversion/qonversion-ios-sdk, minimum 6.13.0).
@objc(DioxusQonversionHost)
public class DioxusQonversionHost: NSObject {
    private static let noCodesEventDelegate = NoCodesEventDelegate()

    /// Initialize Qonversion (Subscription Management) and No-Codes with the same project key.
    ///
    /// - Returns: `nil` on success, or an error description string on failure.
    @objc(initializeWithProjectKey:sandbox:)
    public static func initialize(projectKey: String, sandbox: Bool) -> String? {
        let trimmed = projectKey.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            return "project_key must not be empty"
        }

        let qonversionConfig = Qonversion.Configuration(
            projectKey: trimmed,
            launchMode: .subscriptionManagement
        )
        qonversionConfig.setEnvironment(sandbox ? .sandbox : .production)
        Qonversion.initWithConfig(qonversionConfig)

        let noCodesConfig = NoCodesConfiguration(projectKey: trimmed, delegate: noCodesEventDelegate)
        NoCodes.initialize(with: noCodesConfig)
        NoCodes.shared.set(delegate: noCodesEventDelegate)
        return nil
    }

    /// Present a No-Codes screen by context key (fire-and-present).
    ///
    /// Hops to the main thread because `NoCodes.shared.showScreen` is `@MainActor`.
    ///
    /// - Returns: `nil` on success, or an error description string on failure.
    @objc(showScreenWithContextKey:)
    public static func showScreen(contextKey: String) -> String? {
        let trimmed = contextKey.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            return "context_key must not be empty"
        }
        let present = {
            NoCodes.shared.showScreen(withContextKey: trimmed)
        }
        if Thread.isMainThread {
            present()
        } else {
            DispatchQueue.main.async(execute: present)
        }
        return nil
    }

    /// Load a No-Codes screen by context key without presenting it (ask-first).
    ///
    /// Posts to the main actor and **waits**. Must not be invoked on the main thread.
    ///
    /// - Returns: JSON envelope (`ok:true` + id/context_key, or `ok:false` + error).
    @objc(loadScreenWithContextKey:timeoutMs:)
    public static func loadScreen(contextKey: String, timeoutMs: Int64) -> String {
        if Thread.isMainThread {
            return encodeLoadScreenError("load_screen must not be called on the main thread", screenNotFound: false)
        }
        let trimmed = contextKey.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            return encodeLoadScreenError("context_key must not be empty", screenNotFound: false)
        }

        return awaitOnMain(
            timeoutMs: timeoutMs,
            timeoutValue: encodeLoadScreenError("load screen timed out", screenNotFound: false, timedOut: true)
        ) { complete in
            Task { @MainActor in
                do {
                    let screen = try await NoCodes.shared.loadScreen(withContextKey: trimmed)
                    complete(encodeLoadScreenSuccess(screen))
                } catch {
                    complete(encodeLoadScreenFailure(error))
                }
            }
        }
    }

    /// Identify the Qonversion user with a stable app user id.
    ///
    /// Posts identify to the main queue and **waits** on the calling thread for
    /// the SDK completion. Must not be invoked on the main thread (deadlock).
    /// The Rust serial worker always calls this off-main.
    ///
    /// - Returns: `nil` on success, or an error description string on failure.
    @objc(identifyWithUserId:timeoutMs:)
    public static func identify(userId: String, timeoutMs: Int64) -> String? {
        let trimmed = userId.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            return "user_id must not be empty"
        }
        if Thread.isMainThread {
            return "identify must not be called on the main thread"
        }

        return awaitOnMain(timeoutMs: timeoutMs, timeoutValue: hostTimeoutSentinel as String?) { complete in
            Qonversion.shared().identify(trimmed) { _, error in
                if let error {
                    complete(error.localizedDescription)
                } else {
                    complete(nil)
                }
            }
        }
    }

    /// Fetch Remote Config for `contextKey`, or the empty context key when `contextKey` is nil/blank.
    ///
    /// Posts to the main queue and **waits** on the calling thread for the SDK completion.
    /// Must not be invoked on the main thread (deadlock). The Rust serial worker always
    /// calls this off-main.
    ///
    /// - Returns: JSON envelope string (`ok:true` + payload, or `ok:false` + error). Never nil.
    @objc(remoteConfigWithContextKey:timeoutMs:)
    public static func remoteConfig(contextKey: String?, timeoutMs: Int64) -> String {
        if Thread.isMainThread {
            return encodeEnvelopeError("remote_config must not be called on the main thread")
        }

        let trimmed = contextKey?.trimmingCharacters(in: .whitespacesAndNewlines)
        let key = (trimmed?.isEmpty == false) ? trimmed : nil

        return awaitOnMain(
            timeoutMs: timeoutMs,
            timeoutValue: encodeEnvelopeError("remote config timed out", timedOut: true)
        ) { complete in
            let handler: (Qonversion.RemoteConfig?, Error?) -> Void = { config, error in
                if let error {
                    complete(encodeEnvelopeError(error.localizedDescription))
                } else if let config {
                    complete(encodeRemoteConfigSuccess(config))
                } else {
                    complete(encodeEnvelopeError("remote config returned no data"))
                }
            }
            if let key {
                Qonversion.shared().remoteConfig(contextKey: key, completion: handler)
            } else {
                // No NS_SWIFT_NAME on the empty-key overload: first argument is unlabeled.
                Qonversion.shared().remoteConfig(handler)
            }
        }
    }

    /// Return the current entitlement map as a JSON envelope.
    ///
    /// Posts to the main queue and **waits**. Must not be invoked on the main thread.
    @objc(checkEntitlementsWithTimeoutMs:)
    public static func checkEntitlements(timeoutMs: Int64) -> String {
        entitlementsCall(timeoutMs: timeoutMs, label: "check entitlements") { complete in
            Qonversion.shared().checkEntitlements { entitlements, error in
                if let error {
                    complete(encodeEnvelopeError(error.localizedDescription))
                } else {
                    complete(encodeEntitlementsSuccess(entitlements))
                }
            }
        }
    }

    /// Restore Store purchases and return the entitlement map as a JSON envelope.
    ///
    /// Posts to the main queue and **waits**. Must not be invoked on the main thread.
    @objc(restoreWithTimeoutMs:)
    public static func restore(timeoutMs: Int64) -> String {
        entitlementsCall(timeoutMs: timeoutMs, label: "restore") { complete in
            Qonversion.shared().restore { entitlements, error in
                if let error {
                    complete(encodeEnvelopeError(error.localizedDescription))
                } else {
                    complete(encodeEntitlementsSuccess(entitlements))
                }
            }
        }
    }

    /// Clear the Qonversion user session.
    ///
    /// Hops to the main queue and **waits**.
    ///
    /// - Returns: `nil` on success, or an error description string on failure.
    @objc(logoutWithTimeoutMs:)
    public static func logout(timeoutMs: Int64) -> String? {
        if Thread.isMainThread {
            Qonversion.shared().logout()
            return nil
        }
        return awaitOnMain(timeoutMs: timeoutMs, timeoutValue: hostTimeoutSentinel as String?) { complete in
            Qonversion.shared().logout()
            complete(nil)
        }
    }

    private static let hostTimeoutSentinel = "dioxus_qonversion:timeout"

    private static func awaitOnMain<T>(
        timeoutMs: Int64,
        timeoutValue: T,
        work: @escaping (@escaping (T) -> Void) -> Void
    ) -> T {
        let semaphore = DispatchSemaphore(value: 0)
        let lock = NSLock()
        var finished = false
        var result = timeoutValue
        let complete: (T) -> Void = { value in
            lock.lock()
            defer { lock.unlock() }
            guard !finished else { return }
            finished = true
            result = value
            semaphore.signal()
        }
        DispatchQueue.main.async {
            work(complete)
        }
        let waitMs = timeoutMs < 0 ? 0 : timeoutMs
        if semaphore.wait(timeout: .now() + .milliseconds(Int(waitMs))) == .timedOut {
            complete(timeoutValue)
        }
        return result
    }

    private static func encodeRemoteConfigSuccess(_ config: Qonversion.RemoteConfig) -> String {
        var dict: [String: Any] = ["ok": true]
        if let payload = config.payload, JSONSerialization.isValidJSONObject(payload) {
            dict["payload"] = payload
        } else {
            dict["payload"] = [:]
        }
        dict["source"] = encodeSource(config.source)
        if let experiment = config.experiment {
            dict["experiment"] = encodeExperiment(experiment)
        } else {
            dict["experiment"] = NSNull()
        }
        return stringifyEnvelope(dict)
    }

    private static func encodeEnvelopeError(_ message: String, timedOut: Bool = false) -> String {
        var dict: [String: Any] = ["ok": false, "error": message]
        if timedOut {
            dict["timed_out"] = true
        }
        return stringifyEnvelope(dict)
    }

    private static func entitlementsCall(
        timeoutMs: Int64,
        label: String,
        work: @escaping (@escaping (String) -> Void) -> Void
    ) -> String {
        if Thread.isMainThread {
            return encodeEnvelopeError("\(label) must not be called on the main thread")
        }
        return awaitOnMain(
            timeoutMs: timeoutMs,
            timeoutValue: encodeEnvelopeError("\(label) timed out", timedOut: true)
        ) { complete in
            work(complete)
        }
    }

    private static func encodeEntitlementsSuccess(
        _ entitlements: [String: Qonversion.Entitlement]
    ) -> String {
        var map: [String: Any] = [:]
        for (id, entitlement) in entitlements {
            map[id] = encodeEntitlement(entitlement)
        }
        return stringifyEnvelope(["ok": true, "entitlements": map])
    }

    private static func encodeEntitlement(_ entitlement: Qonversion.Entitlement) -> [String: Any] {
        let productId = entitlement.productID.trimmingCharacters(in: .whitespacesAndNewlines)
        var obj: [String: Any] = [
            "id": entitlement.entitlementID,
            "is_active": entitlement.isActive,
            "product_id": productId.isEmpty ? NSNull() : productId,
            "renew_state": renewStateString(entitlement.renewState),
            "source": entitlementSourceString(entitlement.source),
        ]
        if let expiration = entitlement.expirationDate {
            obj["expiration_date"] = Int64((expiration.timeIntervalSince1970 * 1000.0).rounded())
        } else {
            obj["expiration_date"] = NSNull()
        }
        return obj
    }

    private static func renewStateString(_ state: Qonversion.EntitlementRenewState) -> String {
        switch state {
        case .nonRenewable: return "non_renewable"
        case .willRenew: return "will_renew"
        case .cancelled: return "canceled"
        case .billingIssue: return "billing_issue"
        case .unknown: return "unknown"
        @unknown default: return "unknown"
        }
    }

    private static func entitlementSourceString(_ source: Qonversion.EntitlementSource) -> String {
        switch source {
        case .appStore: return "appstore"
        case .playStore: return "playstore"
        case .stripe: return "stripe"
        case .manual: return "manual"
        case .unknown: return "unknown"
        @unknown default: return "unknown"
        }
    }

    private static func encodeLoadScreenSuccess(_ screen: NoCodesScreen) -> String {
        var dict: [String: Any] = [
            "ok": true,
            "id": screen.id,
            "context_key": screen.contextKey,
            "default_variables": screen.defaultVariables.map(encodeScreenVariable),
        ]
        if let selected = screen.defaultSelectedProductId, !selected.isEmpty {
            dict["default_selected_product_id"] = selected
        } else {
            dict["default_selected_product_id"] = NSNull()
        }
        return stringifyEnvelope(dict)
    }

    private static func encodeScreenVariable(_ variable: NoCodesScreenVariable) -> [String: Any] {
        [
            "kind": screenVariableKindString(variable.kind),
            "key": variable.key,
            "type": variable.type,
            "value": encodeScreenVariableValue(variable.value),
        ]
    }

    private static func screenVariableKindString(_ kind: NoCodesScreenVariableKind) -> String {
        switch kind {
        case .custom: return "custom"
        case .product: return "product"
        case .selectedProduct: return "selected_product"
        case .unknown: return "unknown"
        @unknown default: return "unknown"
        }
    }

    private static func encodeScreenVariableValue(_ value: NoCodesScreenVariableValue) -> Any {
        switch value {
        case .bool(let flag): return flag
        case .string(let text): return text
        case .number(let number): return number
        case .none: return NSNull()
        }
    }

    private static func encodeLoadScreenFailure(_ error: Error) -> String {
        if let noCodesError = error as? NoCodesError {
            return encodeLoadScreenError(
                noCodesError.message,
                screenNotFound: noCodesError.type == .screenNotFound
            )
        }
        return encodeLoadScreenError(error.localizedDescription, screenNotFound: false)
    }

    private static func encodeLoadScreenError(
        _ message: String,
        screenNotFound: Bool,
        timedOut: Bool = false
    ) -> String {
        var dict: [String: Any] = [
            "ok": false,
            "error": message,
            "screen_not_found": screenNotFound,
        ]
        if timedOut {
            dict["timed_out"] = true
        }
        return stringifyEnvelope(dict)
    }

    private static func encodeSource(_ source: Qonversion.RemoteConfigurationSource) -> [String: Any] {
        let contextKey: Any
        if let key = source.contextKey, !key.isEmpty {
            contextKey = key
        } else {
            contextKey = NSNull()
        }
        return [
            "id": source.identifier,
            "name": source.name,
            "assignment_type": assignmentTypeString(source.assignmentType),
            "type": sourceTypeString(source.type),
            "context_key": contextKey,
        ]
    }

    private static func encodeExperiment(_ experiment: Qonversion.Experiment) -> [String: Any] {
        [
            "id": experiment.identifier,
            "name": experiment.name,
            "group": [
                "id": experiment.group.identifier,
                "name": experiment.group.name,
                "type": groupTypeString(experiment.group.type),
            ],
        ]
    }

    private static func sourceTypeString(_ type: Qonversion.RemoteConfigurationSourceType) -> String {
        switch type {
        case .remoteConfiguration: return "remote_configuration"
        case .experimentControlGroup: return "experiment_control_group"
        case .experimentTreatmentGroup: return "experiment_treatment_group"
        case .unknown: return "unknown"
        @unknown default: return "unknown"
        }
    }

    private static func assignmentTypeString(_ type: Qonversion.RemoteConfigurationAssignmentType) -> String {
        switch type {
        case .auto: return "auto"
        case .manual: return "manual"
        case .unknown: return "unknown"
        @unknown default: return "unknown"
        }
    }

    private static func groupTypeString(_ type: Qonversion.ExperimentGroupType) -> String {
        switch type {
        case .control: return "control"
        case .treatment: return "treatment"
        case .unknown: return "unknown"
        @unknown default: return "unknown"
        }
    }

    private static func stringifyEnvelope(_ dict: [String: Any]) -> String {
        stringifyJson(dict) ?? #"{"ok":false,"error":"failed to serialize envelope"}"#
    }
}

private func stringifyJson(_ dict: [String: Any]) -> String? {
    guard JSONSerialization.isValidJSONObject(dict),
          let data = try? JSONSerialization.data(withJSONObject: dict),
          let string = String(data: data, encoding: .utf8)
    else {
        return nil
    }
    return string
}

/// Forwards No-Codes load failures and purchase / restore / finish / custom-action events.
private final class NoCodesEventDelegate: NoCodesDelegate {
    func noCodesFinishedExecuting(action: NoCodesAction) {
        notifyScreenEvent(kind: "action_finished", action: action, message: nil)
    }

    func noCodesFailedToExecute(action: NoCodesAction, error: Error?) {
        let message = error.map { String(describing: $0) } ?? "No-Codes action failed"
        notifyScreenEvent(kind: "action_failed", action: action, message: message)
    }

    func noCodesFinished() {
        notifyScreenEventJson(["kind": "finished"])
    }

    func noCodesReceivedCustomAction(value: String) {
        notifyScreenEventJson(["kind": "custom_action", "value": value])
    }

    func noCodesFailedToLoadScreen(error: Error?) {
        let storeUnavailable = isStoreUnavailable(error)
        let message = error.map { String(describing: $0) } ?? "No-Codes screen failed to load"
        message.withCString { cstr in
            dioxus_qonversion_notify_screen_failed(storeUnavailable ? 1 : 0, cstr)
        }
        NoCodes.shared.close()
    }

    private func notifyScreenEvent(kind: String, action: NoCodesAction, message: String?) {
        var dict: [String: Any] = [
            "kind": kind,
            "action": actionTypeString(action.type),
        ]
        if let message {
            dict["message"] = message
        }
        notifyScreenEventJson(dict)
    }

    private func notifyScreenEventJson(_ dict: [String: Any]) {
        guard let string = stringifyJson(dict) else {
            return
        }
        string.withCString { cstr in
            dioxus_qonversion_notify_screen_event(cstr)
        }
    }

    private func actionTypeString(_ type: NoCodesActionType) -> String {
        switch type {
        case .purchase: return "purchase"
        case .restore: return "restore"
        case .close: return "close"
        case .closeAll: return "close_all"
        case .navigation: return "navigation"
        case .url: return "url"
        case .deeplink: return "deeplink"
        default: return "unknown"
        }
    }
}

private func isStoreUnavailable(_ error: Error?) -> Bool {
    guard let error else {
        return false
    }
    let nsError = error as NSError
    let blob = "\(nsError.domain) \(nsError.code) \(nsError.localizedDescription) \(error)"
        .uppercased()
    if nsError.domain.contains("StoreKit") || nsError.domain.contains("SKError") {
        return true
    }
    return blob.contains("APPLESTOREERROR")
        || blob.contains("STORE IS UNAVAILABLE")
        || blob.contains("STORE UNAVAILABLE")
}
