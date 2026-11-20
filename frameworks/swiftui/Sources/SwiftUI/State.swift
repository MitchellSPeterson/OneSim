#if !os(WASI)
import Foundation
#endif
@_exported import Observation   // `@Observable` / `Observable`, as Apple's SwiftUI re-exports it

final class _Slot<V> { var value: V; init(_ v: V) { value = v } }

protocol _StateProperty { func _bind(_ key: String) }

/// Bind every @State in `view` to the registry slot for (path, type, label).
func _bindState<V>(_ view: V, _ path: String) {
    for ch in Mirror(reflecting: view).children {
        if let s = ch.value as? _StateProperty { s._bind("\(path)#\(V.self).\(ch.label ?? "?")") }
    }
}

@propertyWrapper
public struct State<Value>: _StateProperty {
    final class Storage { var initial: Value; var slot: _Slot<Value>?; init(_ v: Value) { initial = v } }
    let storage: Storage
    public init(wrappedValue: Value) { storage = Storage(wrappedValue) }
    public init(initialValue: Value) { storage = Storage(initialValue) }

    func _bind(_ key: String) {
        let rt = _Runtime.shared
        rt.touched.insert(key)
        if let s = rt.slots[key] as? _Slot<Value> { storage.slot = s; return }
        let s = _Slot(storage.initial)
        rt.slots[key] = s
        storage.slot = s
    }

    public var wrappedValue: Value {
        get { storage.slot?.value ?? storage.initial }
        nonmutating set {
            if let s = storage.slot { s.value = newValue } else { storage.initial = newValue }
            _Runtime.shared.dirty = true
        }
    }
    public var projectedValue: Binding<Value> {
        Binding(get: { self.wrappedValue }, set: { self.wrappedValue = $0 })
    }
}

@propertyWrapper @dynamicMemberLookup
public struct Binding<Value> {
    let getter: () -> Value
    let setter: (Value) -> Void
    public init(get: @escaping () -> Value, set: @escaping (Value) -> Void) { getter = get; setter = set }
    public var wrappedValue: Value { get { getter() } nonmutating set { setter(newValue) } }
    public var projectedValue: Binding<Value> { self }
    public static func constant(_ v: Value) -> Binding<Value> { Binding(get: { v }, set: { _ in }) }
}

extension Binding {
    public subscript<T>(dynamicMember keyPath: WritableKeyPath<Value, T>) -> Binding<T> {
        Binding<T>(get: { self.wrappedValue[keyPath: keyPath] }, set: { self.wrappedValue[keyPath: keyPath] = $0 })
    }
}
extension Binding where Value: MutableCollection {
    public subscript(_ index: Value.Index) -> Binding<Value.Element> {
        Binding<Value.Element>(get: { self.wrappedValue[index] }, set: { self.wrappedValue[index] = $0 })
    }
}

// MARK: - ObservableObject

public final class ObservableObjectPublisher {
    public init() {}
    public func send() { _Runtime.shared.dirty = true }
}
public protocol ObservableObject: AnyObject {
    var objectWillChange: ObservableObjectPublisher { get }
}
extension ObservableObject {
    public var objectWillChange: ObservableObjectPublisher { ObservableObjectPublisher() }
}

/// Any assignment (including in-place mutation) marks the runtime dirty so the host gets a new tree.
@propertyWrapper
public struct Published<Value> {
    public var wrappedValue: Value { didSet { _Runtime.shared.dirty = true } }
    public init(wrappedValue: Value) { self.wrappedValue = wrappedValue }
}

@propertyWrapper
public struct ObservedObject<ObjectType: ObservableObject> {
    @dynamicMemberLookup
    public struct Wrapper {
        let object: ObjectType
        public subscript<T>(dynamicMember keyPath: ReferenceWritableKeyPath<ObjectType, T>) -> Binding<T> {
            let o = object
            return Binding(get: { o[keyPath: keyPath] }, set: { o[keyPath: keyPath] = $0 })
        }
    }
    public var wrappedValue: ObjectType
    public init(wrappedValue: ObjectType) { self.wrappedValue = wrappedValue }
    public var projectedValue: Wrapper { Wrapper(object: wrappedValue) }
}

/// Bindings to the properties of an `@Observable` object (`$model.name`). Writes go through the object, whose
/// observation marks the tree dirty.
@propertyWrapper @dynamicMemberLookup
public struct Bindable<Value> {
    public var wrappedValue: Value
    public init(wrappedValue: Value) where Value: AnyObject & Observable { self.wrappedValue = wrappedValue }
    public init(_ wrappedValue: Value) where Value: AnyObject & Observable { self.wrappedValue = wrappedValue }
    public init(projectedValue: Bindable<Value>) { self = projectedValue }
    public var projectedValue: Bindable<Value> { self }
    public subscript<Subject>(dynamicMember keyPath: ReferenceWritableKeyPath<Value, Subject>) -> Binding<Subject> {
        let o = wrappedValue
        return Binding(get: { o[keyPath: keyPath] }, set: { o[keyPath: keyPath] = $0 })
    }
}

/// Like @State, but the slot holds a reference object created once (autoclosure runs only when the slot is new).
@propertyWrapper
public struct StateObject<ObjectType: ObservableObject>: _StateProperty {
    final class Storage { let make: () -> ObjectType; var slot: _Slot<ObjectType>?; init(_ m: @escaping () -> ObjectType) { make = m } }
    let storage: Storage
    public init(wrappedValue: @autoclosure @escaping () -> ObjectType) { storage = Storage(wrappedValue) }

    func _bind(_ key: String) {
        let rt = _Runtime.shared
        rt.touched.insert(key)
        if let s = rt.slots[key] as? _Slot<ObjectType> { storage.slot = s; return }
        let s = _Slot(storage.make())
        rt.slots[key] = s
        storage.slot = s
    }
    public var wrappedValue: ObjectType {
        if storage.slot == nil { storage.slot = _Slot(storage.make()) }  // read outside evaluation
        return storage.slot!.value
    }
    public var projectedValue: ObservedObject<ObjectType>.Wrapper { .init(object: wrappedValue) }
}

// MARK: - Environment

public enum ColorScheme: CaseIterable, Equatable, Hashable { case light, dark }

public enum DynamicTypeSize: CaseIterable, Hashable, Comparable {
    case xSmall, small, medium, large, xLarge, xxLarge, xxxLarge, accessibility1, accessibility2, accessibility3, accessibility4, accessibility5
    public var isAccessibilitySize: Bool { self >= .accessibility1 }
    public static func < (a: Self, b: Self) -> Bool { allCases.firstIndex(of: a)! < allCases.firstIndex(of: b)! }
}

public struct DismissAction {
    let action: () -> Void
    public func callAsFunction() { action() }
}

public enum UserInterfaceSizeClass: Hashable, Sendable { case compact, regular }

public protocol EnvironmentKey {
    associatedtype Value
    static var defaultValue: Value { get }
}

// Built-in values default to live runtime state; `.environment(_:_:)` overrides them for a subtree.
struct _ColorSchemeKey: EnvironmentKey { static var defaultValue: ColorScheme { _Runtime.shared.appearance == "dark" ? .dark : .light } }
struct _DynamicTypeKey: EnvironmentKey { static var defaultValue: DynamicTypeSize { _Runtime.shared.dynamicType } }
/// iPad (shorter side >= 700 pt): regular both ways. iPhone: vertical is compact in landscape; horizontal is regular
/// only for landscape Plus/Max widths (>= 896 pt).
struct _HSizeKey: EnvironmentKey {
    static var defaultValue: UserInterfaceSizeClass? {
        let (w, h) = _Runtime.shared.viewport
        return min(w, h) >= 700 || (w > h && w >= 896) ? .regular : .compact
    }
}
struct _VSizeKey: EnvironmentKey {
    static var defaultValue: UserInterfaceSizeClass? {
        let (w, h) = _Runtime.shared.viewport
        return min(w, h) >= 700 || h >= w ? .regular : .compact
    }
}

public enum ScenePhase: Comparable, Hashable, Sendable { case background, inactive, active }
struct _ScenePhaseKey: EnvironmentKey { static let defaultValue = ScenePhase.active }

#if !os(WASI)
/// Without a handler (or when it returns `.systemAction`) the URL goes to the system: the guest reports it as a
/// `module.called` event (nothing else opens; the app stays in front).
public struct OpenURLAction {
    public struct Result {
        enum Kind { case handled, discarded, system(URL?) }
        let kind: Kind
        public static let handled = Result(kind: .handled)
        public static let discarded = Result(kind: .discarded)
        public static let systemAction = Result(kind: .system(nil))
        public static func systemAction(_ url: URL) -> Result { Result(kind: .system(url)) }
    }
    let handler: ((URL) -> Result)?
    public init(handler: @escaping (URL) -> Result) { self.handler = handler }
    init() { handler = nil }
    public func callAsFunction(_ url: URL) { callAsFunction(url) { _ in } }
    public func callAsFunction(_ url: URL, completion: @escaping (_ accepted: Bool) -> Void) {
        switch (handler?(url) ?? .systemAction).kind {
        case .handled: completion(true)
        case .discarded: completion(false)
        case .system(let to):
            _send(["type": "event", "name": "module.called",
                   "detail": ["module": "OpenURLAction", "method": "openURL", "args": [(to ?? url).absoluteString]] as Node])
            completion(true)
        }
    }
}
struct _OpenURLKey: EnvironmentKey { static var defaultValue: OpenURLAction { OpenURLAction() } }
extension EnvironmentValues {
    public var openURL: OpenURLAction { get { self[_OpenURLKey.self] } set { self[_OpenURLKey.self] = newValue } }
}
#endif

public struct EnvironmentValues {
    var values: [ObjectIdentifier: Any] = _Runtime.shared.envObjects   // `.environment` values and objects in scope
    var nav: _NavState? = _Runtime.shared.nav   // captured at body time so `dismiss` still works from an action later
    var sheetDismiss: (() -> Void)? = _Runtime.shared.dismissHook
    /// Stored, not keyed: `.disabled` sets it, and `.environment(\.isEnabled, false)` disables controls below.
    public var isEnabled = !_Runtime.shared.disabled
    public init() {}
    public subscript<K: EnvironmentKey>(key: K.Type) -> K.Value {
        get { if let v = values[ObjectIdentifier(key)] { return v as! K.Value }; return K.defaultValue }
        set { values[ObjectIdentifier(key)] = newValue }
    }
    public var colorScheme: ColorScheme { get { self[_ColorSchemeKey.self] } set { self[_ColorSchemeKey.self] = newValue } }
    public var dynamicTypeSize: DynamicTypeSize { get { self[_DynamicTypeKey.self] } set { self[_DynamicTypeKey.self] = newValue } }
    public var horizontalSizeClass: UserInterfaceSizeClass? { get { self[_HSizeKey.self] } set { self[_HSizeKey.self] = newValue } }
    public var verticalSizeClass: UserInterfaceSizeClass? { get { self[_VSizeKey.self] } set { self[_VSizeKey.self] = newValue } }
    public var scenePhase: ScenePhase { get { self[_ScenePhaseKey.self] } set { self[_ScenePhaseKey.self] = newValue } }
    /// Pops a pushed screen, else dismisses the enclosing sheet.
    public var dismiss: DismissAction {
        let n = nav, s = sheetDismiss
        return DismissAction { if let n, n.depth > 0 { n.pop() } else if let s { s() } else { n?.pop() } }
    }
}

/// Snapshotted when the owning view's `body` is about to run (like @State), so closures can use it after evaluation.
@propertyWrapper
public struct Environment<Value>: _StateProperty {
    final class Box { var values: EnvironmentValues? }
    let read: (EnvironmentValues) -> Value
    let box = Box()
    public init(_ keyPath: KeyPath<EnvironmentValues, Value>) { read = { $0[keyPath: keyPath] } }
    /// An `@Observable` object put in scope with `.environment(_:)`.
    public init(_ objectType: Value.Type) where Value: AnyObject & Observable {
        read = {
            guard let o = $0.values[ObjectIdentifier(Value.self)] as? Value else {
                fatalError("No Observable object of type \(Value.self) found. A View.environment(_:) for \(Value.self) may be missing as an ancestor of this view.")
            }
            return o
        }
    }
    public init<T: AnyObject & Observable>(_ objectType: T.Type) where Value == T? {
        read = { $0.values[ObjectIdentifier(T.self)] as? T }
    }
    func _bind(_ key: String) { box.values = EnvironmentValues() }
    public var wrappedValue: Value { read(box.values ?? EnvironmentValues()) }
}

/// Resolved from the evaluation context (`.environmentObject(_:)` on an ancestor) when `body` is about to run.
@propertyWrapper
public struct EnvironmentObject<ObjectType: ObservableObject>: _StateProperty {
    final class Box { var object: ObjectType? }
    let box = Box()
    public init() {}
    func _bind(_ key: String) { box.object = _Runtime.shared.envObjects[ObjectIdentifier(ObjectType.self)] as? ObjectType }
    public var wrappedValue: ObjectType {
        guard let o = box.object else {
            fatalError("No ObservableObject of type \(ObjectType.self) found. A View.environmentObject(_:) for \(ObjectType.self) may be missing as an ancestor of this view.")
        }
        return o
    }
    public var projectedValue: ObservedObject<ObjectType>.Wrapper { .init(object: wrappedValue) }
}

func _withEnv<T>(_ env: [ObjectIdentifier: Any], _ f: () -> T) -> T {
    let rt = _Runtime.shared, saved = rt.envObjects
    rt.envObjects = env
    defer { rt.envObjects = saved }
    return f()
}

struct _LifecycleView<Content: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, onAppear: (() -> Void)?, onDisappear: (() -> Void)?
    func _children(_ path: String) -> [Node] {
        _Runtime.shared.noteLifecycle(path, onAppear, onDisappear)
        return SwiftUI._children(content, path)
    }
}

extension View {
    public func environmentObject<O: ObservableObject>(_ object: O) -> some View {
        _CtxView(content: self) { $0.envObjects[ObjectIdentifier(O.self)] = object }
    }
    public func environment<T: AnyObject & Observable>(_ object: T?) -> some View {
        _CtxView(content: self) { $0.envObjects[ObjectIdentifier(T.self)] = object }
    }
    public func environment<V>(_ keyPath: WritableKeyPath<EnvironmentValues, V>, _ value: V) -> some View {
        _CtxView(content: self) { rt in
            var env = EnvironmentValues()
            env[keyPath: keyPath] = value
            rt.envObjects = env.values
            rt.disabled = !env.isEnabled
        }
    }
    public func onAppear(perform action: (() -> Void)? = nil) -> some View { _LifecycleView(content: self, onAppear: action, onDisappear: nil) }
    public func onDisappear(perform action: (() -> Void)? = nil) -> some View { _LifecycleView(content: self, onAppear: nil, onDisappear: action) }
}

struct _ChangeView<Content: View, V: Equatable>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, value: V, initial: Bool, action: (V, V) -> Void
    func _children(_ path: String) -> [Node] {
        _Runtime.shared.noteChange(path, value, initial: initial, action)
        return SwiftUI._children(content, path)
    }
}

extension View {
    public func onChange<V: Equatable>(of value: V, initial: Bool = false, _ action: @escaping (V, V) -> Void) -> some View {
        _ChangeView(content: self, value: value, initial: initial, action: action)
    }
    public func onChange<V: Equatable>(of value: V, initial: Bool = false, _ action: @escaping () -> Void) -> some View {
        _ChangeView(content: self, value: value, initial: initial) { _, _ in action() }
    }
    public func onChange<V: Equatable>(of value: V, perform action: @escaping (V) -> Void) -> some View {
        _ChangeView(content: self, value: value, initial: false) { _, new in action(new) }
    }

    /// A single tap wraps the content in a plain-style button node (the host's button hit testing and accessibility);
    /// more taps are a `TapGesture` (section L).
    public func onTapGesture(count: Int = 1, perform action: @escaping () -> Void) -> some View {
        _TapView(content: self, count: count, action: action)
    }
}

struct _TapView<Content: View>: View, _Primitive {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, count: Int, action: () -> Void
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared
        // A `GestureMask` without `.subviews` turns this off like any other gesture.
        if rt.gesturesOff { return _eval(content, path) }
        if count > 1 {
            let node = _eval(content, path)
            guard !rt.disabled else { return node }
            let g = TapGesture(count: count).onEnded(action)
            return _attachGesture([node], path, .normal) { g._recognizer($0) }[0]
        }
        return ["kind": "button", "id": path, "action": rt.disabled ? 0 : rt.registerAction(action), "label": _eval(content, path + "/0"),
                "style": "plain", "disabled": rt.disabled]
    }
}

// MARK: - AppStorage

#if os(WASI)
// No Foundation on WASI: an in-memory store (lives for the session, not persisted).
var _defaultsStore: [String: Any] = [:]
func _defaultsGet(_ key: String) -> Any? { _defaultsStore[key] }
func _defaultsSet(_ key: String, _ v: Any) { _defaultsStore[key] = v }
#else
func _defaultsGet(_ key: String) -> Any? { UserDefaults.standard.object(forKey: key) }
func _defaultsSet(_ key: String, _ v: Any) { UserDefaults.standard.set(v, forKey: key) }
#endif

/// Persists in `UserDefaults.standard` (in memory on the web build). Bool/Int/Double/String and RawRepresentable with String/Int raw values.
@propertyWrapper
public struct AppStorage<Value> {
    let key: String, fallback: Value
    let read: (Any) -> Value?, write: (Value) -> Any
    init(_ key: String, _ fallback: Value, read: @escaping (Any) -> Value?, write: @escaping (Value) -> Any) {
        self.key = key; self.fallback = fallback; self.read = read; self.write = write
    }
    public var wrappedValue: Value {
        get { _defaultsGet(key).flatMap(read) ?? fallback }
        nonmutating set { _defaultsSet(key, write(newValue)); _Runtime.shared.dirty = true }
    }
    public var projectedValue: Binding<Value> { Binding(get: { self.wrappedValue }, set: { self.wrappedValue = $0 }) }
}

extension AppStorage {
    public init(wrappedValue: Value, _ key: String) where Value == Bool { self.init(key, wrappedValue, read: { $0 as? Bool }, write: { $0 }) }
    public init(wrappedValue: Value, _ key: String) where Value == Int { self.init(key, wrappedValue, read: { $0 as? Int }, write: { $0 }) }
    public init(wrappedValue: Value, _ key: String) where Value == Double { self.init(key, wrappedValue, read: { $0 as? Double }, write: { $0 }) }
    public init(wrappedValue: Value, _ key: String) where Value == String { self.init(key, wrappedValue, read: { $0 as? String }, write: { $0 }) }
    public init(wrappedValue: Value, _ key: String) where Value: RawRepresentable, Value.RawValue == String {
        self.init(key, wrappedValue, read: { ($0 as? String).flatMap(Value.init(rawValue:)) }, write: { $0.rawValue })
    }
    public init(wrappedValue: Value, _ key: String) where Value: RawRepresentable, Value.RawValue == Int {
        self.init(key, wrappedValue, read: { ($0 as? Int).flatMap(Value.init(rawValue:)) }, write: { $0.rawValue })
    }
}

// MARK: - SceneStorage

// One scene per session: a key maps to one value app-wide while the guest runs (not restored after relaunch).
var _sceneStore: [String: Any] = [:]

/// Same value types as `AppStorage`, kept in memory for the session.
@propertyWrapper
public struct SceneStorage<Value> {
    let key: String, fallback: Value
    let read: (Any) -> Value?, write: (Value) -> Any
    init(_ key: String, _ fallback: Value, read: @escaping (Any) -> Value?, write: @escaping (Value) -> Any) {
        self.key = key; self.fallback = fallback; self.read = read; self.write = write
    }
    public var wrappedValue: Value {
        get { _sceneStore[key].flatMap(read) ?? fallback }
        nonmutating set { _sceneStore[key] = write(newValue); _Runtime.shared.dirty = true }
    }
    public var projectedValue: Binding<Value> { Binding(get: { self.wrappedValue }, set: { self.wrappedValue = $0 }) }
}

extension SceneStorage {
    public init(wrappedValue: Value, _ key: String) where Value == Bool { self.init(key, wrappedValue, read: { $0 as? Bool }, write: { $0 }) }
    public init(wrappedValue: Value, _ key: String) where Value == Int { self.init(key, wrappedValue, read: { $0 as? Int }, write: { $0 }) }
    public init(wrappedValue: Value, _ key: String) where Value == Double { self.init(key, wrappedValue, read: { $0 as? Double }, write: { $0 }) }
    public init(wrappedValue: Value, _ key: String) where Value == String { self.init(key, wrappedValue, read: { $0 as? String }, write: { $0 }) }
    public init(wrappedValue: Value, _ key: String) where Value: RawRepresentable, Value.RawValue == String {
        self.init(key, wrappedValue, read: { ($0 as? String).flatMap(Value.init(rawValue:)) }, write: { $0.rawValue })
    }
    public init(wrappedValue: Value, _ key: String) where Value: RawRepresentable, Value.RawValue == Int {
        self.init(key, wrappedValue, read: { ($0 as? Int).flatMap(Value.init(rawValue:)) }, write: { $0.rawValue })
    }
}

// MARK: - ScaledMetric

/// Scales with the Dynamic Type size like `UIFontMetrics.scaledValue(for:)`: by the text style's point size at the
/// current size over its size at `.large`.
@propertyWrapper
public struct ScaledMetric<Value: BinaryFloatingPoint> {
    let base: Value, style: Font.TextStyle
    public init(wrappedValue: Value, relativeTo textStyle: Font.TextStyle) { base = wrappedValue; style = textStyle }
    public init(wrappedValue: Value) { self.init(wrappedValue: wrappedValue, relativeTo: .body) }
    public var wrappedValue: Value {
        let i = DynamicTypeSize.allCases.firstIndex(of: _Runtime.shared.dynamicType)!
        let row = style.sizes
        return Value(Double(base) * row[i] / row[3])
    }
}
