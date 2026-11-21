import SwiftUI

// Wire-test app (#49): a WindowGroup(for:) without a default value opens with a nil binding it can then set.
// No `import Foundation`: NSObject and UserDefaults come with SwiftUI, as with Apple's. A plain delegate (neither
// ObservableObject nor @Observable) is still the app's delegate.

final class PlainDelegate: NSObject, UIApplicationDelegate {
    var launched = false
    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        launched = true
        return true
    }
}

@main
struct SceneOptLab: App {
    @UIApplicationDelegateAdaptor var delegate: PlainDelegate
    var body: some Scene {
        WindowGroup(for: String.self) { $value in
            VStack {
                Text("Delegate \(delegate.launched) \(UIApplication.shared.delegate === delegate)")
                Text("Value \(value ?? "none")")
                Button("Set") { value = "set" }
                Button("Clear") { value = nil }
            }
        }
        .defaultAppStorage(UserDefaults.standard)
    }
}
