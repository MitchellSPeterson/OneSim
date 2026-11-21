#if !os(WASI)
import Foundation
#endif
internal import OneSimBridge

// MARK: - Scene and App

public protocol Scene {
    associatedtype Body: Scene
    @SceneBuilder var body: Body { get }
}
extension Never: Scene {}

/// Scenes OneSim evaluates directly. `_window` returns the window's root node, or nil when the scene puts up no
/// window on iPhone. Scene state lives under `path` ("app/..."); the window's views keep the root path "0".
protocol _PrimitiveScene { func _window(_ path: String) -> Node? }

func _sceneWindow<S: Scene>(_ s: S, _ path: String) -> Node? {
    if let p = s as? _PrimitiveScene { return p._window(path) }
    _bindState(s, path)
    return _sceneWindow(s.body, path)
}

/// An iPhone has a single window: the first scene that makes one (`WindowGroup`) gets it; the rest are never shown.
public struct _TupleScene<each S: Scene>: Scene, _PrimitiveScene {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let scenes: (repeat each S)
    init(_ scenes: repeat each S) { self.scenes = (repeat each scenes) }
    func _window(_ path: String) -> Node? {
        var found: Node? = nil, i = 0
        func visit<T: Scene>(_ s: T) {
            defer { i += 1 }
            if found == nil { found = _sceneWindow(s, "\(path)/\(i)") }
        }
        repeat visit(each scenes)
        return found
    }
}

public struct _EmptyScene: Scene, _PrimitiveScene {
    public typealias Body = Never
    public var body: Never { fatalError() }
    func _window(_ path: String) -> Node? { nil }
}

public protocol _LimitedAvailabilitySceneMarker {}
public struct _LimitedAvailabilityScene: Scene, _PrimitiveScene, _LimitedAvailabilitySceneMarker {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let scene: (any Scene)?
    func _window(_ path: String) -> Node? {
        guard let scene else { return nil }
        return _sceneWindow(scene, path)
    }
}

@resultBuilder
public struct SceneBuilder {
    public static func buildExpression<Content: Scene>(_ content: Content) -> Content { content }
    public static func buildBlock() -> _EmptyScene { _EmptyScene() }
    public static func buildBlock<Content: Scene>(_ content: Content) -> Content { content }
    @_disfavoredOverload
    public static func buildBlock<each Content: Scene>(_ content: repeat each Content) -> _TupleScene<repeat each Content> {
        _TupleScene(repeat each content)
    }
    /// `if #available` around a scene (the only `if` a scene builder accepts).
    public static func buildOptional(_ scene: (any Scene & _LimitedAvailabilitySceneMarker)?) -> _LimitedAvailabilityScene {
        _LimitedAvailabilityScene(scene: scene)
    }
    public static func buildLimitedAvailability(_ scene: some Scene) -> any Scene & _LimitedAvailabilitySceneMarker {
        _LimitedAvailabilityScene(scene: scene)
    }
}

public protocol App {
    associatedtype Body: Scene
    init()
    @SceneBuilder var body: Body { get }
}

#if os(WASI)
public typealias _Done = Void  // reactor: launching registers the app and returns
#else
public typealias _Done = Never
#endif

extension App {
    /// Like `UIApplicationMain`: makes the app once (its delegate adaptor with it), runs the delegate's launch
    /// callbacks while inactive, then shows the first scene's window. The app's own state lives under "app".
    public static func main() {
        let app = Self()
        _launchApp(app)
    }
}

func _launchApp<A: App>(_ app: A) -> _Done {
    let rt = _Runtime.shared
    rt.scenePhase = .inactive
    #if !os(WASI)
    _appDidLaunch()
    #endif
    return _onesimStart {
        _bindState(app, "app")
        #if !os(WASI)
        _injectAppDelegate()
        #endif
        return _sceneWindow(app.body, "app/body") ?? ["kind": "empty", "id": "0"]
    }
}

extension Group: Scene, _PrimitiveScene where Content: Scene {
    @_disfavoredOverload
    public init(@SceneBuilder content: () -> Content) { self.content = content() }
    func _window(_ path: String) -> Node? { _sceneWindow(content, path) }
}

// MARK: - WindowGroup

public struct WindowGroup<Content: View>: Scene, _PrimitiveScene {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let content: Content?, make: (() -> Content)?
    /// Title and id name the window on iPad and Mac; an iPhone shows neither.
    let title: Text?, id: String?
    init(content: Content, title: Text?, id: String?) { self.content = content; make = nil; self.title = title; self.id = id }

    public init(@ViewBuilder content: () -> Content) { self.init(content: content(), title: nil, id: nil) }
    public init(id: String, @ViewBuilder content: () -> Content) { self.init(content: content(), title: nil, id: id) }
    @_disfavoredOverload
    public init(_ title: Text, @ViewBuilder content: () -> Content) { self.init(content: content(), title: title, id: nil) }
    @_disfavoredOverload
    public init(_ title: Text, id: String, @ViewBuilder content: () -> Content) { self.init(content: content(), title: title, id: id) }
    @_disfavoredOverload
    public init<S: StringProtocol>(_ title: S, @ViewBuilder content: () -> Content) { self.init(content: content(), title: Text(String(title)), id: nil) }
    @_disfavoredOverload
    public init<S: StringProtocol>(_ title: S, id: String, @ViewBuilder content: () -> Content) {
        self.init(content: content(), title: Text(String(title)), id: id)
    }

    func _window(_ path: String) -> Node? { _withEditMode("0") { _eval(make?() ?? content!, "0") } }
}

extension WindowGroup {
    /// `makeContent` runs each time the window's content is built (the App re-renders), not once at init.
    init(make: @escaping () -> Content, title: Text?, id: String?) { self.content = nil; self.make = make; self.title = title; self.id = id }
    public init(@ViewBuilder makeContent: @escaping () -> Content) { self.init(make: makeContent, title: nil, id: nil) }
    public init(id: String, @ViewBuilder makeContent: @escaping () -> Content) { self.init(make: makeContent, title: nil, id: id) }
    @_disfavoredOverload
    public init(_ title: Text, @ViewBuilder makeContent: @escaping () -> Content) { self.init(make: makeContent, title: title, id: nil) }
    @_disfavoredOverload
    public init(_ title: Text, id: String, @ViewBuilder makeContent: @escaping () -> Content) { self.init(make: makeContent, title: title, id: id) }
    @_disfavoredOverload
    public init<S: StringProtocol>(_ title: S, @ViewBuilder makeContent: @escaping () -> Content) {
        self.init(make: makeContent, title: Text(String(title)), id: nil)
    }
    @_disfavoredOverload
    public init<S: StringProtocol>(_ title: S, id: String, @ViewBuilder makeContent: @escaping () -> Content) {
        self.init(make: makeContent, title: Text(String(title)), id: id)
    }
}

/// The window value of a data-presenting `WindowGroup`. On iPhone the one window opens without a presented value
/// (or with `defaultValue`); the binding is the window's state.
public struct PresentedWindowContent<Data: Decodable & Encodable & Hashable, Content: View>: View {
    final class Box { var value: Data?; var seeded = false }
    @State private var box = Box()
    let defaultValue: (() -> Data)?
    let make: (Binding<Data?>) -> Content
    public var body: some View {
        let b = box
        if !b.seeded { b.seeded = true; b.value = defaultValue?() }
        return make(Binding(get: { b.value }, set: { b.value = $0; _Runtime.shared.markDirty() }))
    }
}

extension WindowGroup {
    init<D, C>(_ title: Text?, _ id: String?, _ def: (() -> D)?, _ make: @escaping (Binding<D?>) -> C)
    where Content == PresentedWindowContent<D, C> {
        self.init(content: PresentedWindowContent(defaultValue: def, make: make), title: title, id: id)
    }
    static func _required<D, C>(_ content: @escaping (Binding<D>) -> C) -> (Binding<D?>) -> C {
        { b in content(Binding(get: { b.wrappedValue! }, set: { b.wrappedValue = $0 })) }
    }

    public init<D, C>(for type: D.Type, @ViewBuilder content: @escaping (Binding<D?>) -> C) where Content == PresentedWindowContent<D, C> {
        self.init(nil, nil, nil, content)
    }
    public init<D, C>(id: String, for type: D.Type, @ViewBuilder content: @escaping (Binding<D?>) -> C) where Content == PresentedWindowContent<D, C> {
        self.init(nil, id, nil, content)
    }
    @_disfavoredOverload
    public init<D, C>(_ title: Text, for type: D.Type, @ViewBuilder content: @escaping (Binding<D?>) -> C) where Content == PresentedWindowContent<D, C> {
        self.init(title, nil, nil, content)
    }
    @_disfavoredOverload
    public init<D, C>(_ title: Text, id: String, for type: D.Type, @ViewBuilder content: @escaping (Binding<D?>) -> C)
    where Content == PresentedWindowContent<D, C> {
        self.init(title, id, nil, content)
    }
    @_disfavoredOverload
    public init<S: StringProtocol, D, C>(_ title: S, for type: D.Type, @ViewBuilder content: @escaping (Binding<D?>) -> C)
    where Content == PresentedWindowContent<D, C> {
        self.init(Text(String(title)), nil, nil, content)
    }
    @_disfavoredOverload
    public init<S: StringProtocol, D, C>(_ title: S, id: String, for type: D.Type, @ViewBuilder content: @escaping (Binding<D?>) -> C)
    where Content == PresentedWindowContent<D, C> {
        self.init(Text(String(title)), id, nil, content)
    }

    public init<D, C>(for type: D.Type = D.self, @ViewBuilder content: @escaping (Binding<D>) -> C, defaultValue: @escaping () -> D)
    where Content == PresentedWindowContent<D, C> {
        self.init(nil, nil, defaultValue, Self._required(content))
    }
    public init<D, C>(id: String, for type: D.Type = D.self, @ViewBuilder content: @escaping (Binding<D>) -> C, defaultValue: @escaping () -> D)
    where Content == PresentedWindowContent<D, C> {
        self.init(nil, id, defaultValue, Self._required(content))
    }
    @_disfavoredOverload
    public init<D, C>(_ title: Text, for type: D.Type = D.self, @ViewBuilder content: @escaping (Binding<D>) -> C, defaultValue: @escaping () -> D)
    where Content == PresentedWindowContent<D, C> {
        self.init(title, nil, defaultValue, Self._required(content))
    }
    @_disfavoredOverload
    public init<D, C>(_ title: Text, id: String, for type: D.Type = D.self, @ViewBuilder content: @escaping (Binding<D>) -> C,
                      defaultValue: @escaping () -> D) where Content == PresentedWindowContent<D, C> {
        self.init(title, id, defaultValue, Self._required(content))
    }
    @_disfavoredOverload
    public init<S: StringProtocol, D, C>(_ title: S, for type: D.Type = D.self, @ViewBuilder content: @escaping (Binding<D>) -> C,
                                         defaultValue: @escaping () -> D) where Content == PresentedWindowContent<D, C> {
        self.init(Text(String(title)), nil, defaultValue, Self._required(content))
    }
    @_disfavoredOverload
    public init<S: StringProtocol, D, C>(_ title: S, id: String, for type: D.Type = D.self, @ViewBuilder content: @escaping (Binding<D>) -> C,
                                         defaultValue: @escaping () -> D) where Content == PresentedWindowContent<D, C> {
        self.init(Text(String(title)), id, defaultValue, Self._required(content))
    }
}

// MARK: - Scene modifiers

/// A scene modifier: `around` runs the inner scene's evaluation (possibly inside a changed context).
public struct _ModifiedScene<S: Scene>: Scene, _PrimitiveScene {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let scene: S
    let around: (String, () -> Node?) -> Node?
    func _window(_ path: String) -> Node? { around(path) { _sceneWindow(scene, path) } }
}

extension Scene {
    func _modified(_ around: @escaping (String, () -> Node?) -> Node?) -> _ModifiedScene<Self> { _ModifiedScene(scene: self, around: around) }
    /// Same context rules as `View` modifiers (`_CtxView`): changes apply to the window below and are undone after.
    func _context(_ apply: @escaping (_Runtime) -> Void) -> _ModifiedScene<Self> {
        _modified { _, inner in
            let rt = _Runtime.shared
            let saved = (rt.envObjects, rt.envLocal, rt.disabled, rt.appStorage)
            defer { (rt.envObjects, rt.envLocal, rt.disabled, rt.appStorage) = saved }
            apply(rt)
            return inner()
        }
    }
    /// Accepted for source compatibility; an iPhone window has no such behaviour.
    func _inert() -> _ModifiedScene<Self> { _modified { _, inner in inner() } }

    public func onChange<V: Equatable>(of value: V, initial: Bool = false, _ action: @escaping (_ oldValue: V, _ newValue: V) -> Void) -> some Scene {
        _modified { path, inner in _Runtime.shared.noteChange(path, value, initial: initial, action); return inner() }
    }
    public func onChange<V: Equatable>(of value: V, initial: Bool = false, _ action: @escaping () -> Void) -> some Scene {
        onChange(of: value, initial: initial) { _, _ in action() }
    }

    public func environment<V>(_ keyPath: WritableKeyPath<EnvironmentValues, V>, _ value: V) -> some Scene {
        _context { rt in _setEnvironment(rt) { $0[keyPath: keyPath] = value } }
    }
    public func transformEnvironment<V>(_ keyPath: WritableKeyPath<EnvironmentValues, V>, transform: @escaping (inout V) -> Void) -> some Scene {
        _context { rt in _setEnvironment(rt) { transform(&$0[keyPath: keyPath]) } }
    }
    public func environment<T: AnyObject & Observable>(_ object: T?) -> some Scene {
        _context { $0.envObjects[ObjectIdentifier(T.self)] = object; $0.envLocal.insert(ObjectIdentifier(T.self)) }
    }
    public func environmentObject<T: ObservableObject>(_ object: T) -> some Scene {
        _context { $0.envObjects[ObjectIdentifier(T.self)] = object; $0.envLocal.insert(ObjectIdentifier(T.self)) }
    }
    /// The store `@AppStorage` uses in this scene's views when they name none.
    public func defaultAppStorage(_ store: UserDefaults) -> some Scene { _context { $0.appStorage = store } }

    /// One scene on iPhone: every URL and user activity reaches it, whatever the conditions.
    public func handlesExternalEvents(matching conditions: Set<String>) -> some Scene { _inert() }
    /// OneSim draws no home indicator on iPhones (the Duo's stays), so hiding it changes nothing on screen.
    public func persistentSystemOverlays(_ preferredVisibility: Visibility) -> some Scene { _inert() }
    public func defaultSize(_ size: CGSize) -> some Scene { _inert() }
    public func defaultSize(width: CGFloat, height: CGFloat) -> some Scene { _inert() }
    public func windowResizability(_ resizability: WindowResizability) -> some Scene { _inert() }
    public func restorationBehavior(_ behavior: SceneRestorationBehavior) -> some Scene { _inert() }
    public func windowManagerRole(_ role: WindowManagerRole) -> some Scene { _inert() }
    public func windowToolbarLabelStyle(fixed: ToolbarLabelStyle) -> some Scene { _inert() }
    public func windowToolbarLabelStyle(_ toolbarLabelStyle: Binding<ToolbarLabelStyle>) -> some Scene { _inert() }
    /// Menu-bar and keyboard-shortcut menus exist on iPad and Mac only. The builder still runs, as SwiftUI does.
    public func commands<Content: Commands>(@CommandsBuilder content: () -> Content) -> some Scene { _ = content(); return _inert() }
    public func commandsRemoved() -> some Scene { _inert() }
    public func commandsReplaced<Content: Commands>(@CommandsBuilder content: () -> Content) -> some Scene { _ = content(); return _inert() }
}

/// Write environment values the way `View.environment(_:_:)` does.
func _setEnvironment(_ rt: _Runtime, _ edit: (inout EnvironmentValues) -> Void) {
    var env = EnvironmentValues()
    edit(&env)
    rt.envObjects = env.values
    rt.envLocal.formUnion(env.setKeys)
    rt.disabled = rt.disabled || !env.isEnabled
}

extension View {
    public func transformEnvironment<V>(_ keyPath: WritableKeyPath<EnvironmentValues, V>, transform: @escaping (inout V) -> Void) -> some View {
        _CtxView(content: self) { rt in _setEnvironment(rt) { transform(&$0[keyPath: keyPath]) } }
    }
    /// The store `@AppStorage` uses in this view's subtree when it names none.
    public func defaultAppStorage(_ store: UserDefaults) -> some View {
        _CtxView(content: self) { $0.appStorage = store }
    }
}

public struct WindowResizability: Sendable {
    let name: String
    public static var automatic: WindowResizability { .init(name: "automatic") }
    public static var contentSize: WindowResizability { .init(name: "contentSize") }
    public static var contentMinSize: WindowResizability { .init(name: "contentMinSize") }
}

public struct SceneRestorationBehavior: Sendable {
    let name: String
    public static let automatic = SceneRestorationBehavior(name: "automatic")
    public static let disabled = SceneRestorationBehavior(name: "disabled")
}

public struct WindowManagerRole: Sendable {
    let name: String
    public static let automatic = WindowManagerRole(name: "automatic")
    public static let principal = WindowManagerRole(name: "principal")
    public static let associated = WindowManagerRole(name: "associated")
}

public struct ToolbarLabelStyle: Sendable, Equatable {
    let name: String
    public static var automatic: ToolbarLabelStyle { .init(name: "automatic") }
    public static var titleAndIcon: ToolbarLabelStyle { .init(name: "titleAndIcon") }
    public static var iconOnly: ToolbarLabelStyle { .init(name: "iconOnly") }
    public static var titleOnly: ToolbarLabelStyle { .init(name: "titleOnly") }
}

// MARK: - Commands

/// Menu commands. iPhones show no menus, so commands only need to build.
public protocol Commands {
    associatedtype Body: Commands
    @CommandsBuilder var body: Body { get }
}
extension Never: Commands {}

public struct EmptyCommands: Commands {
    public typealias Body = Never
    public var body: Never { fatalError() }
    public init() {}
}

public struct CommandGroupPlacement: Sendable {
    let name: String
    public static let appInfo = Self(name: "appInfo"), appSettings = Self(name: "appSettings")
    public static let systemServices = Self(name: "systemServices"), appVisibility = Self(name: "appVisibility")
    public static let appTermination = Self(name: "appTermination"), newItem = Self(name: "newItem")
    public static let saveItem = Self(name: "saveItem"), importExport = Self(name: "importExport")
    public static let printItem = Self(name: "printItem"), undoRedo = Self(name: "undoRedo")
    public static let pasteboard = Self(name: "pasteboard"), textEditing = Self(name: "textEditing")
    public static let textFormatting = Self(name: "textFormatting"), toolbar = Self(name: "toolbar")
    public static let sidebar = Self(name: "sidebar"), windowSize = Self(name: "windowSize")
    public static let windowList = Self(name: "windowList"), singleWindowList = Self(name: "singleWindowList")
    public static let windowArrangement = Self(name: "windowArrangement"), help = Self(name: "help")
}

public struct CommandMenu<Content: View>: Commands {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let name: Text, content: Content
    public init(_ name: Text, @ViewBuilder content: () -> Content) { self.name = name; self.content = content() }
    @_disfavoredOverload
    public init<S: StringProtocol>(_ name: S, @ViewBuilder content: () -> Content) { self.init(Text(String(name)), content: content) }
}

public struct CommandGroup<Content: View>: Commands {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let placement: CommandGroupPlacement, content: Content
    public init(before group: CommandGroupPlacement, @ViewBuilder addition: () -> Content) { placement = group; content = addition() }
    public init(after group: CommandGroupPlacement, @ViewBuilder addition: () -> Content) { placement = group; content = addition() }
    public init(replacing group: CommandGroupPlacement, @ViewBuilder addition: () -> Content) { placement = group; content = addition() }
}

/// The standard command sets (sidebar, toolbar, text editing and formatting, inspector).
public struct SidebarCommands: Commands { public typealias Body = Never; public var body: Never { fatalError() }; public init() {} }
public struct ToolbarCommands: Commands { public typealias Body = Never; public var body: Never { fatalError() }; public init() {} }
public struct TextEditingCommands: Commands { public typealias Body = Never; public var body: Never { fatalError() }; public init() {} }
public struct TextFormattingCommands: Commands { public typealias Body = Never; public var body: Never { fatalError() }; public init() {} }
public struct InspectorCommands: Commands { public typealias Body = Never; public var body: Never { fatalError() }; public init() {} }

public struct _TupleCommands<each C: Commands>: Commands {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let content: (repeat each C)
}
public struct _OptionalCommands<C: Commands>: Commands {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let content: C?
}
public struct _ConditionalCommands<T: Commands, F: Commands>: Commands {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let first: T?, second: F?
}
public struct _AnyCommands: Commands {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let content: any Commands
}

@resultBuilder
public struct CommandsBuilder {
    public static func buildExpression<Content: Commands>(_ content: Content) -> Content { content }
    public static func buildBlock() -> EmptyCommands { EmptyCommands() }
    public static func buildBlock<C: Commands>(_ content: C) -> C { content }
    @_disfavoredOverload
    public static func buildBlock<each C: Commands>(_ content: repeat each C) -> _TupleCommands<repeat each C> {
        _TupleCommands(content: (repeat each content))
    }
    public static func buildIf<C: Commands>(_ content: C?) -> _OptionalCommands<C> { _OptionalCommands(content: content) }
    public static func buildEither<T: Commands, F: Commands>(first: T) -> _ConditionalCommands<T, F> { .init(first: first, second: nil) }
    public static func buildEither<T: Commands, F: Commands>(second: F) -> _ConditionalCommands<T, F> { .init(first: nil, second: second) }
    public static func buildLimitedAvailability(_ content: any Commands) -> _AnyCommands { _AnyCommands(content: content) }
}

// MARK: - Scene phase and URLs

/// Host `scene_phase` line: move to that phase (the host steps through `inactive`), post the UIKit notifications,
/// and re-render. Always answers a tree.
func _applyScenePhase(_ name: String) {
    let rt = _Runtime.shared
    let phases: [String: ScenePhase] = ["active": .active, "inactive": .inactive, "background": .background]
    guard let phase = phases[name] else {
        "scene_phase: unknown phase \"\(name)\"".withCString { simcore_report_error($0) }
        rt.submitTree()
        return
    }
    let old = rt.scenePhase
    rt.scenePhase = phase
    #if !os(WASI)
    _postPhaseChange(old, phase)
    #endif
    rt.submitTree()
}

/// The scene connected and became active (launch; the web build has no notifications to post).
func _sceneDidBecomeActive() {
    let rt = _Runtime.shared, old = rt.scenePhase
    rt.scenePhase = .active
    #if !os(WASI)
    _postPhaseChange(old, .active)
    #endif
}

#if !os(WASI)
/// Host `open_url` line: every `onOpenURL` in the window runs, in view order. Always answers a tree.
func _applyOpenURL(_ string: String) {
    let rt = _Runtime.shared
    if let url = URL(string: string) {
        for h in rt.urlHandlers { h(url) }
    } else {
        "open_url: not a URL: \"\(string)\"".withCString { simcore_report_error($0) }
    }
    rt.submitTree()
}

struct _OpenURLView<Content: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, action: (URL) -> Void
    func _children(_ path: String) -> [Node] {
        _Runtime.shared.urlHandlers.append(action)
        return SwiftUI._children(content, path)
    }
}

extension View {
    /// Runs when the system opens a URL in the app (the host's `open_url`). Every such view in the window gets it.
    public func onOpenURL(perform action: @escaping (URL) -> ()) -> some View { _OpenURLView(content: self, action: action) }
}
#endif
