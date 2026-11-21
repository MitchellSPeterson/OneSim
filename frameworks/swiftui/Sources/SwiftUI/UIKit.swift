#if !os(WASI)
import Foundation

// The UIKit app-lifecycle types SwiftUI apps use (`@UIApplicationDelegateAdaptor`), declared in OneSim's SwiftUI
// module. A SwiftUI app is scene-based, so (as on iOS) the delegate gets launch and termination callbacks while
// phase changes arrive as `scenePhase` and the `UIApplication` notifications.

public enum UIApplicationState: Int, Sendable { case active, inactive, background }

open class UIApplication {
    public typealias State = UIApplicationState

    public struct LaunchOptionsKey: Hashable, RawRepresentable, Sendable {
        public let rawValue: String
        public init(rawValue: String) { self.rawValue = rawValue }
        public init(_ rawValue: String) { self.rawValue = rawValue }
        public static let url = Self("UIApplicationLaunchOptionsURLKey")
        public static let sourceApplication = Self("UIApplicationLaunchOptionsSourceApplicationKey")
        public static let remoteNotification = Self("UIApplicationLaunchOptionsRemoteNotificationKey")
        public static let annotation = Self("UIApplicationLaunchOptionsAnnotationKey")
        public static let location = Self("UIApplicationLaunchOptionsLocationKey")
        public static let shortcutItem = Self("UIApplicationLaunchOptionsShortcutItemKey")
        public static let userActivityDictionary = Self("UIApplicationLaunchOptionsUserActivityDictionaryKey")
        public static let userActivityType = Self("UIApplicationLaunchOptionsUserActivityTypeKey")
    }

    public struct OpenURLOptionsKey: Hashable, RawRepresentable, Sendable {
        public let rawValue: String
        public init(rawValue: String) { self.rawValue = rawValue }
        public init(_ rawValue: String) { self.rawValue = rawValue }
        public static let sourceApplication = Self("UIApplicationOpenURLOptionsSourceApplicationKey")
        public static let annotation = Self("UIApplicationOpenURLOptionsAnnotationKey")
        public static let openInPlace = Self("UIApplicationOpenURLOptionsOpenInPlaceKey")
    }

    nonisolated(unsafe) public static let shared = UIApplication()
    init() {}

    /// Follows the scene phase (host `scene_phase`).
    open var applicationState: State {
        switch _Runtime.shared.scenePhase {
        case .active: .active
        case .inactive: .inactive
        case .background: .background
        }
    }
    /// The `@UIApplicationDelegateAdaptor` instance, if the app declares one.
    open var delegate: UIApplicationDelegate? { _appDelegate }

    public static let didFinishLaunchingNotification = Notification.Name("UIApplicationDidFinishLaunchingNotification")
    public static let didBecomeActiveNotification = Notification.Name("UIApplicationDidBecomeActiveNotification")
    public static let willResignActiveNotification = Notification.Name("UIApplicationWillResignActiveNotification")
    public static let didEnterBackgroundNotification = Notification.Name("UIApplicationDidEnterBackgroundNotification")
    public static let willEnterForegroundNotification = Notification.Name("UIApplicationWillEnterForegroundNotification")
    public static let willTerminateNotification = Notification.Name("UIApplicationWillTerminateNotification")
}

/// UIKit's app delegate. Every method is optional (default implementations below). With SwiftUI's scene-based
/// lifecycle iOS calls only the launch and termination methods; the `application...Active` / `...Background`
/// pair and `application(_:open:options:)` are declared so delegates compile, and are never called.
public protocol UIApplicationDelegate: AnyObject {
    func application(_ application: UIApplication, willFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?) -> Bool
    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?) -> Bool
    func applicationWillTerminate(_ application: UIApplication)
    func applicationDidBecomeActive(_ application: UIApplication)
    func applicationWillResignActive(_ application: UIApplication)
    func applicationDidEnterBackground(_ application: UIApplication)
    func applicationWillEnterForeground(_ application: UIApplication)
    func applicationDidReceiveMemoryWarning(_ application: UIApplication)
    func application(_ app: UIApplication, open url: URL, options: [UIApplication.OpenURLOptionsKey: Any]) -> Bool
}

extension UIApplicationDelegate {
    public func application(_ application: UIApplication, willFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?) -> Bool { true }
    public func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?) -> Bool { true }
    public func applicationWillTerminate(_ application: UIApplication) {}
    public func applicationDidBecomeActive(_ application: UIApplication) {}
    public func applicationWillResignActive(_ application: UIApplication) {}
    public func applicationDidEnterBackground(_ application: UIApplication) {}
    public func applicationWillEnterForeground(_ application: UIApplication) {}
    public func applicationDidReceiveMemoryWarning(_ application: UIApplication) {}
    public func application(_ app: UIApplication, open url: URL, options: [UIApplication.OpenURLOptionsKey: Any]) -> Bool { false }
}

nonisolated(unsafe) var _appDelegate: UIApplicationDelegate?
/// Puts the delegate in the environment each pass: as an `ObservableObject` and/or an `@Observable` object.
nonisolated(unsafe) var _appDelegateInject: ((_Runtime) -> Void)?

/// Creates the app delegate when the `App` is made (once). Only one adaptor per app, as SwiftUI requires.
@propertyWrapper
public struct UIApplicationDelegateAdaptor<DelegateType: NSObject & UIApplicationDelegate> {
    public let wrappedValue: DelegateType

    init(_ make: () -> DelegateType, _ inject: ((_Runtime) -> Void)?) {
        guard _appDelegate == nil else {
            fatalError("UIApplicationDelegateAdaptor: an app can declare only one delegate adaptor (\(DelegateType.self)).")
        }
        let d = make()
        wrappedValue = d
        _appDelegate = d
        _appDelegateInject = inject
    }
    public init(_ delegateType: DelegateType.Type = DelegateType.self) { self.init({ DelegateType() }, nil) }
}

extension UIApplicationDelegateAdaptor where DelegateType: ObservableObject {
    public init(_ delegateType: DelegateType.Type = DelegateType.self) {
        var made: DelegateType?
        self.init({ let d = DelegateType(); made = d; return d }) { rt in
            rt.envObjects[ObjectIdentifier(DelegateType.self)] = made
        }
    }
    public var projectedValue: ObservedObject<DelegateType>.Wrapper { .init(object: wrappedValue) }
}

extension UIApplicationDelegateAdaptor where DelegateType: Observation.Observable {
    public init(_ delegateType: DelegateType.Type = DelegateType.self) {
        var made: DelegateType?
        self.init({ let d = DelegateType(); made = d; return d }) { rt in rt.envObjects[ObjectIdentifier(DelegateType.self)] = made }
    }
}

func _injectAppDelegate() { _appDelegateInject?(_Runtime.shared) }

/// `UIApplicationMain` after the app is made: launch callbacks while inactive, then the launch notification.
func _appDidLaunch() {
    let app = UIApplication.shared
    if let d = _appDelegate {
        _ = d.application(app, willFinishLaunchingWithOptions: nil)
        _ = d.application(app, didFinishLaunchingWithOptions: nil)
    }
    NotificationCenter.default.post(name: UIApplication.didFinishLaunchingNotification, object: app)
}

/// The notifications for each step between two phases (the host sends one step at a time).
func _postPhaseChange(_ old: ScenePhase, _ new: ScenePhase) {
    let app = UIApplication.shared, center = NotificationCenter.default
    var names: [Notification.Name] = []
    switch (old, new) {
    case (.active, .inactive): names = [UIApplication.willResignActiveNotification]
    case (.active, .background): names = [UIApplication.willResignActiveNotification, UIApplication.didEnterBackgroundNotification]
    case (.inactive, .background): names = [UIApplication.didEnterBackgroundNotification]
    case (.background, .inactive): names = [UIApplication.willEnterForegroundNotification]
    case (.background, .active): names = [UIApplication.willEnterForegroundNotification, UIApplication.didBecomeActiveNotification]
    case (.inactive, .active): names = [UIApplication.didBecomeActiveNotification]
    default: break
    }
    _pool { for n in names { center.post(name: n, object: app) } }
}

/// The guest is shutting down. iOS tells an app it terminates only while it is not suspended in the background.
func _appWillTerminate() {
    guard _Runtime.shared.scenePhase != .background else { return }
    let app = UIApplication.shared
    _pool {
        _appDelegate?.applicationWillTerminate(app)
        NotificationCenter.default.post(name: UIApplication.willTerminateNotification, object: app)
    }
}
#endif
