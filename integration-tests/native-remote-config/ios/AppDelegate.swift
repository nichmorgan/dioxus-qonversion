import UIKit
import DioxusQonversionHost

// Lifecycle only: no UIWindow, view controller, Dioxus runtime, or WebView.
@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    func application(_ application: UIApplication,
                     didFinishLaunchingWithOptions options: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
        // A concrete runtime reference retains/registers the existing Swift host.
        precondition(!NSStringFromClass(DioxusQonversionHost.self).isEmpty)
        return true
    }
}
