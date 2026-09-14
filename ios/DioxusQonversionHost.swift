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
}
