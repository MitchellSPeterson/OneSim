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

@propertyWrapper
public struct Binding<Value> {
    let getter: () -> Value
    let setter: (Value) -> Void
    public init(get: @escaping () -> Value, set: @escaping (Value) -> Void) { getter = get; setter = set }
    public var wrappedValue: Value { get { getter() } nonmutating set { setter(newValue) } }
    public var projectedValue: Binding<Value> { self }
    public static func constant(_ v: Value) -> Binding<Value> { Binding(get: { v }, set: { _ in }) }
}
