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
}
