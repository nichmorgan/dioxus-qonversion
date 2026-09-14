import Foundation
import Qonversion
import NoCodes

/// Thin ObjC-visible host so Rust can call Qonversion + No-Codes via `objc`.
///
/// Compile this file into your Dioxus iOS target and add the Qonversion iOS SDK
/// (SPM: https://github.com/qonversion/qonversion-ios-sdk, minimum 6.13.0).
@objc(DioxusQonversionHost)
public class DioxusQonversionHost: NSObject {
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

        let noCodesConfig = NoCodesConfiguration(projectKey: trimmed)
        NoCodes.initialize(with: noCodesConfig)
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

    /// Identify the Qonversion user with a stable app user id.
    ///
    /// Posts identify to the main queue and **waits** on the calling thread for
    /// the SDK completion. Must not be invoked on the main thread (deadlock).
    /// The Rust serial worker always calls this off-main.
    ///
    /// - Returns: `nil` on success, or an error description string on failure.
    @objc(identifyWithUserId:)
    public static func identify(userId: String) -> String? {
        let trimmed = userId.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            return "user_id must not be empty"
        }
        if Thread.isMainThread {
            return "identify must not be called on the main thread"
        }

        let semaphore = DispatchSemaphore(value: 0)
        var errorMessage: String?
        DispatchQueue.main.async {
            Qonversion.shared().identify(trimmed) { _, error in
                if let error {
                    errorMessage = error.localizedDescription
                }
                semaphore.signal()
            }
        }
        semaphore.wait()
        return errorMessage
    }

    /// Fetch Remote Config for `contextKey`, or the empty context key when `contextKey` is nil/blank.
    ///
    /// Posts to the main queue and **waits** on the calling thread for the SDK completion.
    /// Must not be invoked on the main thread (deadlock). The Rust serial worker always
    /// calls this off-main.
    ///
    /// - Returns: JSON envelope string (`ok:true` + payload, or `ok:false` + error). Never nil.
    @objc(remoteConfigWithContextKey:)
    public static func remoteConfig(contextKey: String?) -> String {
        if Thread.isMainThread {
            return encodeRemoteConfigError("remote_config must not be called on the main thread")
        }

        let trimmed = contextKey?.trimmingCharacters(in: .whitespacesAndNewlines)
        let key = (trimmed?.isEmpty == false) ? trimmed : nil

        let semaphore = DispatchSemaphore(value: 0)
        var envelope = encodeRemoteConfigError("remote config did not complete")
        DispatchQueue.main.async {
            let handler: (Qonversion.RemoteConfig?, Error?) -> Void = { config, error in
                if let error {
                    envelope = encodeRemoteConfigError(error.localizedDescription)
                } else if let config {
                    envelope = encodeRemoteConfigSuccess(config)
                } else {
                    envelope = encodeRemoteConfigError("remote config returned no data")
                }
                semaphore.signal()
            }
            if let key {
                Qonversion.shared().remoteConfig(contextKey: key, completion: handler)
            } else {
                // No NS_SWIFT_NAME on the empty-key overload: first argument is unlabeled.
                Qonversion.shared().remoteConfig(handler)
            }
        }
        semaphore.wait()
        return envelope
    }

    /// Clear the Qonversion user session.
    ///
    /// Hops to the main queue and **waits**.
    ///
    /// - Returns: `nil` on success, or an error description string on failure.
    @objc(logout)
    public static func logout() -> String? {
        return runOnMainSync {
            Qonversion.shared().logout()
            return nil
        }
    }

    private static func runOnMainSync<T>(_ block: () -> T) -> T {
        if Thread.isMainThread {
            return block()
        }
        var result: T!
        let semaphore = DispatchSemaphore(value: 0)
        DispatchQueue.main.async {
            result = block()
            semaphore.signal()
        }
        semaphore.wait()
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

    private static func encodeRemoteConfigError(_ message: String) -> String {
        stringifyEnvelope(["ok": false, "error": message])
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
        guard JSONSerialization.isValidJSONObject(dict),
              let data = try? JSONSerialization.data(withJSONObject: dict),
              let string = String(data: data, encoding: .utf8)
        else {
            return #"{"ok":false,"error":"failed to serialize remote config"}"#
        }
        return string
    }
}
