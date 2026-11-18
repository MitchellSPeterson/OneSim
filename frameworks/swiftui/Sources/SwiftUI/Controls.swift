#if !os(WASI)
import Foundation
#endif

public enum Axis: Int8, CaseIterable {
    case horizontal = 1, vertical = 2
    public struct Set: OptionSet {
        public let rawValue: Int8
        public init(rawValue: Int8) { self.rawValue = rawValue }
        public static let horizontal = Set(rawValue: 1)
        public static let vertical = Set(rawValue: 2)
    }
}

public struct ScrollView<Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let axes: Axis.Set, content: Content
    public init(_ axes: Axis.Set = .vertical, showsIndicators: Bool = true, @ViewBuilder content: () -> Content) {
        self.axes = axes; self.content = content()
    }
    func _emit(_ path: String) -> Node {
        ["kind": "scroll", "id": path, "axis": axes.contains(.horizontal) && !axes.contains(.vertical) ? "horizontal" : "vertical",
         "child": _eval(content, path + "/0")]
    }
}

public struct Divider: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    public init() {}
    func _emit(_ path: String) -> Node { ["kind": "divider", "id": path] }
}

// MARK: - Lists

public struct ForEach<Data: RandomAccessCollection, ID: Hashable, Content: View>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let data: Data, id: KeyPath<Data.Element, ID>, content: (Data.Element) -> Content
    public init(_ data: Data, id: KeyPath<Data.Element, ID>, @ViewBuilder content: @escaping (Data.Element) -> Content) {
        self.data = data; self.id = id; self.content = content
    }
    // Element id goes into the structural path so @State follows items, not positions.
    func _children(_ path: String) -> [Node] {
        data.flatMap { SwiftUI._children(content($0), "\(path)/\($0[keyPath: id])") }
    }
}
extension ForEach where ID == Data.Element.ID, Data.Element: Identifiable {
    public init(_ data: Data, @ViewBuilder content: @escaping (Data.Element) -> Content) { self.init(data, id: \.id, content: content) }
}
extension ForEach where Data == Range<Int>, ID == Int {
    public init(_ data: Range<Int>, @ViewBuilder content: @escaping (Int) -> Content) { self.init(data, id: \.self, content: content) }
}

public struct Group<Content: View>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let content: Content
    public init(@ViewBuilder content: () -> Content) { self.content = content() }
    func _children(_ path: String) -> [Node] { SwiftUI._children(content, path) }
}

public struct Section<Content: View>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let header: String?, content: Content
    public init(@ViewBuilder content: () -> Content) { header = nil; self.content = content() }
    public init(_ header: String, @ViewBuilder content: () -> Content) { self.header = header; self.content = content() }
    func _children(_ path: String) -> [Node] {
        var n: Node = ["kind": "_section", "rows": SwiftUI._children(content, path)]
        if let header { n["header"] = header }
        return [n]
    }
}

public struct List<Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let content: Content
    public init(@ViewBuilder content: () -> Content) { self.content = content() }
    func _emit(_ path: String) -> Node {
        var sections: [Node] = [], loose: [Node] = []
        for n in _children(content, path) {
            if n["kind"] as? String == "_section" {
                if !loose.isEmpty { sections.append(["rows": loose]); loose = [] }
                var s = n; s["kind"] = nil; sections.append(s)
            } else { loose.append(n) }
        }
        if !loose.isEmpty { sections.append(["rows": loose]) }
        return ["kind": "list", "id": path, "style": "insetGrouped", "sections": sections]
    }
}

extension List {
    public init<Data: RandomAccessCollection, ID: Hashable, RowContent: View>(
        _ data: Data, id: KeyPath<Data.Element, ID>, @ViewBuilder rowContent: @escaping (Data.Element) -> RowContent
    ) where Content == ForEach<Data, ID, RowContent> {
        self.init { ForEach(data, id: id, content: rowContent) }
    }
    public init<Data: RandomAccessCollection, RowContent: View>(
        _ data: Data, @ViewBuilder rowContent: @escaping (Data.Element) -> RowContent
    ) where Content == ForEach<Data, Data.Element.ID, RowContent>, Data.Element: Identifiable {
        self.init { ForEach(data, content: rowContent) }
    }
    public init<RowContent: View>(_ data: Range<Int>, @ViewBuilder rowContent: @escaping (Int) -> RowContent)
    where Content == ForEach<Range<Int>, Int, RowContent> {
        self.init { ForEach(data, content: rowContent) }
    }
}

// MARK: - Controls

public struct TextField: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let placeholder: String, text: Binding<String>
    public init(_ placeholder: String, text: Binding<String>) { self.placeholder = placeholder; self.text = text }
    func _emit(_ path: String) -> Node {
        let b = text
        return ["kind": "text_field", "id": path, "placeholder": placeholder, "text": b.wrappedValue,
                "field": _Runtime.shared.registerField { b.wrappedValue = $0 }]
    }
}

public struct Toggle<Label: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let isOn: Binding<Bool>, label: Label
    public init(isOn: Binding<Bool>, @ViewBuilder label: () -> Label) { self.isOn = isOn; self.label = label() }
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared, b = isOn
        return ["kind": "toggle", "id": path, "isOn": b.wrappedValue, "disabled": rt.disabled,
                "action": rt.disabled ? 0 : rt.registerAction { b.wrappedValue.toggle() }, "label": _eval(label, path + "/0")]
    }
}
extension Toggle where Label == Text {
    public init(_ title: String, isOn: Binding<Bool>) { self.init(isOn: isOn) { Text(title) } }
}

public struct Stepper<Label: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let label: Label
    let dec: (() -> Void)?, inc: (() -> Void)?
    public init<V: Strideable>(value: Binding<V>, in bounds: ClosedRange<V>, step: V.Stride = 1, @ViewBuilder label: () -> Label) {
        self.label = label()
        dec = value.wrappedValue > bounds.lowerBound ? { value.wrappedValue = max(bounds.lowerBound, value.wrappedValue.advanced(by: -step)) } : nil
        inc = value.wrappedValue < bounds.upperBound ? { value.wrappedValue = min(bounds.upperBound, value.wrappedValue.advanced(by: step)) } : nil
    }
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared
        func act(_ f: (() -> Void)?) -> Any { f.flatMap { rt.disabled ? nil : rt.registerAction($0) } ?? NSNull() }
        return ["kind": "stepper", "id": path, "label": _eval(label, path + "/0"), "disabled": rt.disabled,
                "decrement": act(dec), "increment": act(inc)]
    }
}
extension Stepper where Label == Text {
    public init<V: Strideable>(_ title: String, value: Binding<V>, in bounds: ClosedRange<V>, step: V.Stride = 1) {
        self.init(value: value, in: bounds, step: step) { Text(title) }
    }
}

// MARK: - Navigation

final class _NavScreen {
    let build: (String) -> Node
    init(_ b: @escaping (String) -> Node) { build = b }
}

/// Type-erased `Binding<[T]>` / `Binding<NavigationPath>` that owns the stack's path.
struct _NavPath {
    let count: () -> Int, element: (Int) -> Any, append: (Any) -> Void, removeLast: () -> Void
}

public struct NavigationPath {
    var items: [Any] = []
    public init() {}
    public var count: Int { items.count }
    public var isEmpty: Bool { items.isEmpty }
    public mutating func append<V: Hashable>(_ value: V) { items.append(value) }
    public mutating func removeLast(_ k: Int = 1) { items.removeLast(k) }
}

/// Guest-owned path stack, persisted in the state registry. Root is depth 0; pushed screens have ids `<stack>/s<depth>`.
/// With `NavigationStack(path:)` the binding is the source of truth for depth and screens are built from its values.
final class _NavState {
    var stack: [_NavScreen] = []
    var titles: [Int: String] = [:]   // last evaluated title per depth, for the back label
    var destinations: [ObjectIdentifier: (Any, String) -> Node] = [:]   // kept across evaluations: hidden screens register nothing
    var ext: _NavPath?
    var depth: Int { ext?.count() ?? stack.count }
    func push(_ s: _NavScreen) { stack.append(s); _Runtime.shared.dirty = true }   // destination links; not tracked in path-bound stacks
    func pushValue(_ v: Any) {
        if let ext { ext.append(v) } else { stack.append(_NavScreen { [self] in build(v, $0) }) }
        _Runtime.shared.dirty = true
    }
    func screen(_ i: Int, _ path: String) -> Node { ext.map { build($0.element(i), path) } ?? stack[i].build(path) }
    func build(_ v: Any, _ path: String) -> Node {
        destinations[ObjectIdentifier(type(of: v))]?(v, path) ?? ["kind": "empty", "id": path]   // no destination: blank screen
    }
    func pop() {
        if let ext { if ext.count() > 0 { ext.removeLast() } } else if !stack.isEmpty { stack.removeLast() }
        _Runtime.shared.dirty = true
    }
}

public struct NavigationStack<Root: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let root: Root
    let path: _NavPath?
    public init(@ViewBuilder root: () -> Root) { self.root = root(); path = nil }
    public init(path: Binding<NavigationPath>, @ViewBuilder root: () -> Root) {
        self.root = root()
        self.path = _NavPath(count: { path.wrappedValue.count }, element: { path.wrappedValue.items[$0] },
                             append: { path.wrappedValue.items.append($0) }, removeLast: { path.wrappedValue.removeLast() })
    }
    public init<D: RandomAccessCollection & RangeReplaceableCollection>(path: Binding<D>, @ViewBuilder root: () -> Root) where D.Element: Hashable {
        self.root = root()
        self.path = _NavPath(count: { path.wrappedValue.count },
                             element: { let a = path.wrappedValue; return a[a.index(a.startIndex, offsetBy: $0)] },
                             append: { if let e = $0 as? D.Element { path.wrappedValue.append(e) } },
                             removeLast: { path.wrappedValue.removeLast() })
    }

    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared
        let key = "\(path)#NavigationStack"
        rt.touched.insert(key)
        let st = (rt.slots[key] as? _NavState) ?? _NavState()
        rt.slots[key] = st
        st.ext = self.path
        let depth = st.depth
        // Screens below the top are not evaluated; keep their @State alive.
        for d in 0..<depth {
            let prefix = d == 0 ? "\(path)/0" : "\(path)/s\(d)"
            for k in rt.slots.keys where k.hasPrefix(prefix + "/") || k.hasPrefix(prefix + "#") { rt.touched.insert(k) }
        }
        let saved = (rt.nav, rt.disabled, rt.buttonStyle)
        rt.nav = st; rt.disabled = false; rt.buttonStyle = "automatic"
        defer { (rt.nav, rt.disabled, rt.buttonStyle) = saved }
        var content = depth == 0 ? _eval(root, "\(path)/0") : st.screen(depth - 1, "\(path)/s\(depth)")

        var title: String?, mode = "large"
        let mods = (content["modifiers"] as? [Node]) ?? []
        for m in mods {
            if let t = m["navigationTitle"] as? String { title = t }
            if let d = m["navigationBarTitleDisplayMode"] as? String { mode = d }
        }
        content["modifiers"] = mods.filter { $0["navigationBarTitleDisplayMode"] == nil }
        st.titles[depth] = title
        var nav: Node = ["kind": "navigation", "id": path, "displayMode": mode, "content": content]
        if let title { nav["title"] = title }
        if depth > 0 {
            nav["back"] = ["action": rt.registerAction { st.pop() }, "label": st.titles[depth - 1] ?? "Back"] as Node
        }
        return nav
    }
}

struct _DestinationView<Content: View, T: Hashable, D: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, destination: (T) -> D
    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared, env = rt.envObjects, dest = destination
        rt.nav?.destinations[ObjectIdentifier(T.self)] = { v, p in _withEnv(env) { _eval(dest(v as! T), p) } }
        return SwiftUI._children(content, path)
    }
}
extension View {
    public func navigationDestination<T: Hashable, D: View>(for data: T.Type, @ViewBuilder destination: @escaping (T) -> D) -> some View {
        _DestinationView(content: self, destination: destination)
    }
}

public struct NavigationLink<Label: View, Destination: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let destination: Destination?, push: ((_NavState) -> Void)?, label: Label   // push set for value links
    public init(@ViewBuilder destination: () -> Destination, @ViewBuilder label: () -> Label) {
        self.destination = destination(); push = nil; self.label = label()
    }
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared, nav = rt.nav, env = rt.envObjects
        let action: () -> Void
        if let push { action = { if let nav { push(nav) } } }
        else { let d = destination!; action = { nav?.push(_NavScreen { p in _withEnv(env) { _eval(d, p) } }) } }
        return ["kind": "button", "id": path, "style": "navigationLink", "disabled": rt.disabled,
                "action": rt.disabled ? 0 : rt.registerAction(action), "label": _eval(label, path + "/0")]
    }
}
extension NavigationLink where Label == Text {
    public init(_ title: String, @ViewBuilder destination: () -> Destination) {
        self.init(destination: destination) { Text(title) }
    }
}
extension NavigationLink where Destination == Never {
    public init<P: Hashable>(value: P?, @ViewBuilder label: () -> Label) {
        destination = nil; self.label = label()
        push = { n in if let value { n.pushValue(value) } }
    }
}
extension NavigationLink where Label == Text, Destination == Never {
    public init<P: Hashable>(_ title: String, value: P?) { self.init(value: value) { Text(title) } }
}
