import Foundation

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

/// Guest-owned path stack, persisted in the state registry. Root is depth 0; pushed screens have ids `<stack>/s<depth>`.
final class _NavState {
    var stack: [_NavScreen] = []
    var titles: [Int: String] = [:]   // last evaluated title per depth, for the back label
    func push(_ s: _NavScreen) { stack.append(s); _Runtime.shared.dirty = true }
    func pop() { if !stack.isEmpty { stack.removeLast(); _Runtime.shared.dirty = true } }
}

public struct NavigationStack<Root: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let root: Root
    public init(@ViewBuilder root: () -> Root) { self.root = root() }

    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared
        let key = "\(path)#NavigationStack"
        rt.touched.insert(key)
        let st = (rt.slots[key] as? _NavState) ?? _NavState()
        rt.slots[key] = st
        let depth = st.stack.count
        // Screens below the top are not evaluated; keep their @State alive.
        for d in 0..<depth {
            let prefix = d == 0 ? "\(path)/0" : "\(path)/s\(d)"
            for k in rt.slots.keys where k.hasPrefix(prefix + "/") || k.hasPrefix(prefix + "#") { rt.touched.insert(k) }
        }
        let saved = (rt.nav, rt.disabled, rt.buttonStyle)
        rt.nav = st; rt.disabled = false; rt.buttonStyle = "automatic"
        defer { (rt.nav, rt.disabled, rt.buttonStyle) = saved }
        var content = depth == 0 ? _eval(root, "\(path)/0") : st.stack[depth - 1].build("\(path)/s\(depth)")

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

public struct NavigationLink<Label: View, Destination: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let destination: Destination, label: Label
    public init(@ViewBuilder destination: () -> Destination, @ViewBuilder label: () -> Label) {
        self.destination = destination(); self.label = label()
    }
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared, d = destination, nav = rt.nav
        let action: () -> Void = { nav?.push(_NavScreen { _eval(d, $0) }) }
        return ["kind": "button", "id": path, "style": "navigationLink", "disabled": rt.disabled,
                "action": rt.disabled ? 0 : rt.registerAction(action), "label": _eval(label, path + "/0")]
    }
}
extension NavigationLink where Label == Text {
    public init(_ title: String, @ViewBuilder destination: () -> Destination) {
        self.init(destination: destination) { Text(title) }
    }
}
