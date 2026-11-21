import Foundation  // Combine comes with it on macOS: ObservableObject/Published are qualified below
import SwiftUI

// Wire-test app for scenes and the app lifecycle (#49): App-level state, scene modifiers, scenePhase, the
// UIApplicationDelegateAdaptor, onOpenURL and defaultAppStorage.

final class LabDelegate: NSObject, UIApplicationDelegate, SwiftUI.ObservableObject {
    @SwiftUI.Published var log: [String] = []
    var observers: [NSObjectProtocol] = []

    func application(_ application: UIApplication, willFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        log.append("will:\(application.applicationState == .inactive ? "inactive" : "other")")
        return true
    }

    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        log.append("did:\(launchOptions == nil ? "nil" : "opts")")
        let center = NotificationCenter.default
        let names: [(Notification.Name, String)] = [
            (UIApplication.willResignActiveNotification, "resign"), (UIApplication.didEnterBackgroundNotification, "bg"),
            (UIApplication.willEnterForegroundNotification, "fg"), (UIApplication.didBecomeActiveNotification, "active"),
        ]
        for (name, tag) in names {
            observers.append(center.addObserver(forName: name, object: nil, queue: nil) { [weak self] _ in self?.log.append(tag) })
        }
        return true
    }

    // Scene-based apps never get these on iOS; SwiftUI apps are scene-based.
    func applicationDidBecomeActive(_ application: UIApplication) { log.append("WRONG-didBecomeActive") }
    func applicationDidEnterBackground(_ application: UIApplication) { log.append("WRONG-didEnterBackground") }

    func applicationWillTerminate(_ application: UIApplication) {
        FileHandle.standardError.write(Data("lab: willTerminate\n".utf8))
    }
}

@Observable final class LabModel { var opened: [String] = [] }
final class LabLegacy: SwiftUI.ObservableObject { @SwiftUI.Published var n = 1 }

struct LabThemeKey: EnvironmentKey { static let defaultValue = "plain" }
extension EnvironmentValues {
    var labTheme: String { get { self[LabThemeKey.self] } set { self[LabThemeKey.self] = newValue } }
}

let labSuite = "onesim.scenelab.suite"
let labInner = "onesim.scenelab.inner"

/// A custom scene: its own @Environment and @State live on the scene.
struct LabScene: Scene {
    @Binding var appTaps: Int
    let model: LabModel
    @Environment(\.scenePhase) var phase
    @State var sceneLog: [String] = []

    var body: some Scene {
        WindowGroup("Lab", id: "main") {
            LabRoot(appTaps: $appTaps, sceneLog: sceneLog)
        }
        .transformEnvironment(\.labTheme) { $0 += "!" }   // inner: runs on the value set outside it
        .environment(\.labTheme, "night")
        .environment(model)
        .environmentObject(LabLegacy())
        .defaultAppStorage(UserDefaults(suiteName: labSuite)!)
        .onChange(of: phase) { old, new in sceneLog.append("\(old)>\(new)") }
        .onChange(of: appTaps, initial: true) { sceneLog.append("taps") }
        .handlesExternalEvents(matching: ["*"])
        .defaultSize(width: 400, height: 800)
        .defaultSize(CGSize(width: 1, height: 2))
        .windowResizability(.contentSize)
        .restorationBehavior(.disabled)
        .windowManagerRole(.principal)
        .windowToolbarLabelStyle(fixed: .iconOnly)
        .windowToolbarLabelStyle(.constant(.titleAndIcon))
        .persistentSystemOverlays(.hidden)
        .commands {
            CommandMenu("Tools") { Button("Run") {} }
            CommandGroup(after: .newItem) { Button("Extra") {} }
            CommandGroup(replacing: .help) { EmptyView() }
            SidebarCommands()
            ToolbarCommands()
            TextEditingCommands()
            if appTaps > 5 { InspectorCommands() }
        }
        .commandsRemoved()
        .commandsReplaced { TextFormattingCommands() }
    }
}

@main
struct SceneLab: App {
    @UIApplicationDelegateAdaptor(LabDelegate.self) var delegate
    @Environment(\.scenePhase) var phase
    @State var appTaps = 0
    @State var model = LabModel()

    init() {
        UserDefaults.standard.removeObject(forKey: "lab.count")
        UserDefaults.standard.removeObject(forKey: "lab.explicit")
        UserDefaults.standard.removePersistentDomain(forName: labSuite)
        UserDefaults.standard.removePersistentDomain(forName: labInner)
    }

    var body: some Scene {
        LabScene(appTaps: $appTaps, model: model)
            .onChange(of: phase) { _, new in if new == .background { model.opened.append("app-saw-background") } }
        // Only the first scene gets a window on iPhone.
        WindowGroup(for: String.self) { $value in Text("Secondary \(value ?? "nil")") }
    }
}

struct LabRoot: View {
    @Binding var appTaps: Int
    let sceneLog: [String]
    @Environment(\.scenePhase) var phase
    @Environment(\.labTheme) var theme
    @Environment(LabModel.self) var model
    @EnvironmentObject var legacy: LabLegacy
    @EnvironmentObject var delegate: LabDelegate
    @AppStorage("lab.count") var count = 0
    @AppStorage("lab.explicit", store: .standard) var explicit = 0

    var stateName: String {
        switch UIApplication.shared.applicationState {
        case .active: "active"
        case .inactive: "inactive"
        case .background: "background"
        @unknown default: "?"
        }
    }

    var body: some View {
        VStack {
          VStack {
            Text("App taps: \(appTaps)")
            Button("Tap app") { appTaps += 1 }
            Text("Phase: \(phase)")
            Text("UIKit: \(stateName)")
            Text("Scene log: \(sceneLog.joined(separator: ","))")
            Text("Theme: \(theme)")
            Text("Legacy: \(legacy.n)")
            Text("Delegate: \(delegate.log.joined(separator: ","))")
            Text("Same delegate: \(UIApplication.shared.delegate === delegate)")
            Text("Opened: \(model.opened.joined(separator: ","))")
          }
          VStack {
            Text("Stored: \(count) std:\(UserDefaults.standard.integer(forKey: "lab.count")) suite:\(UserDefaults(suiteName: labSuite)!.integer(forKey: "lab.count"))")
            Button("Store") { count += 1; explicit += 1 }
            Text("Explicit: \(explicit) std:\(UserDefaults.standard.integer(forKey: "lab.explicit"))")
            LabInner()
                .defaultAppStorage(UserDefaults(suiteName: labInner)!)
                .transformEnvironment(\.labTheme) { $0 = $0.uppercased() }
          }
        }
        .onOpenURL { url in model.opened.append(url.absoluteString) }
    }
}

struct LabInner: View {
    @AppStorage("lab.count") var count = 0
    @Environment(\.labTheme) var theme
    @Environment(LabModel.self) var model
    var body: some View {
        VStack {
            Text("Inner: \(count) inner:\(UserDefaults(suiteName: labInner)!.integer(forKey: "lab.count")) theme:\(theme)")
            Button("Store inner") { count += 10 }
        }
        .onOpenURL { url in model.opened.append("inner:\(url.host ?? "-")") }
    }
}
