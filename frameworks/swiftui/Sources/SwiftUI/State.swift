#if !os(WASI)
import Foundation
#endif

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

public struct EnvironmentValues {
    var nav: _NavState? = _Runtime.shared.nav   // captured at body time so `dismiss` still works from an action later
    public init() {}
    public var colorScheme: ColorScheme { _Runtime.shared.appearance == "dark" ? .dark : .light }
    public var dynamicTypeSize: DynamicTypeSize { _Runtime.shared.dynamicType }
    public var dismiss: DismissAction { let n = nav; return DismissAction { n?.pop() } }
}

/// Snapshotted when the owning view's `body` is about to run (like @State), so closures can use it after evaluation.
@propertyWrapper
public struct Environment<Value>: _StateProperty {
    final class Box { var values: EnvironmentValues? }
    let keyPath: KeyPath<EnvironmentValues, Value>
    let box = Box()
    public init(_ keyPath: KeyPath<EnvironmentValues, Value>) { self.keyPath = keyPath }
    func _bind(_ key: String) { box.values = EnvironmentValues() }
    public var wrappedValue: Value { (box.values ?? EnvironmentValues())[keyPath: keyPath] }
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

func _withEnv<T>(_ env: [ObjectIdentifier: AnyObject], _ f: () -> T) -> T {
    let rt = _Runtime.shared, saved = rt.envObjects
    rt.envObjects = env
    defer { rt.envObjects = saved }
    return f()
}

struct _EnvObjectView<Content: View, O: ObservableObject>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, object: O
    func _children(_ path: String) -> [Node] {
        _withEnv(_Runtime.shared.envObjects.merging([ObjectIdentifier(O.self): object]) { $1 }) { SwiftUI._children(content, path) }
    }
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
    public func environmentObject<O: ObservableObject>(_ object: O) -> some View { _EnvObjectView(content: self, object: object) }
    public func onAppear(perform action: (() -> Void)? = nil) -> some View { _LifecycleView(content: self, onAppear: action, onDisappear: nil) }
    public func onDisappear(perform action: (() -> Void)? = nil) -> some View { _LifecycleView(content: self, onAppear: nil, onDisappear: action) }
}
