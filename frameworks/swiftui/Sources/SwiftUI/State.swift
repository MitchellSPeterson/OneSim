import Foundation

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

public struct EnvironmentValues {
    public init() {}
    public var colorScheme: ColorScheme { _Runtime.shared.appearance == "dark" ? .dark : .light }
}

/// Read live at `body` time, so an `environment` message followed by re-evaluation is enough.
@propertyWrapper
public struct Environment<Value> {
    let keyPath: KeyPath<EnvironmentValues, Value>
    public init(_ keyPath: KeyPath<EnvironmentValues, Value>) { self.keyPath = keyPath }
    public var wrappedValue: Value { EnvironmentValues()[keyPath: keyPath] }
}
