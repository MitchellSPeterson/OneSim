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
    let axes: Axis.Set, showsIndicators: Bool, content: Content
    public init(_ axes: Axis.Set = .vertical, showsIndicators: Bool = true, @ViewBuilder content: () -> Content) {
        self.axes = axes; self.showsIndicators = showsIndicators; self.content = content()
    }
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared
        rt.scrollStack.append(path)
        defer { rt.scrollStack.removeLast() }
        return ["kind": "scroll", "id": path,
                "axis": axes.contains(.horizontal) && !axes.contains(.vertical) ? "horizontal" : "vertical",
                "indicators": showsIndicators ? "automatic" : "hidden",
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

public struct Group<Content: View>: View, _Flattenable, _ModifierTransparent {
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
    let placeholder: String, text: Binding<String>, secure: Bool
    public init(_ placeholder: String, text: Binding<String>) { self.placeholder = placeholder; self.text = text; secure = false }
    init(_ placeholder: String, text: Binding<String>, secure: Bool) { self.placeholder = placeholder; self.text = text; self.secure = secure }
    func _emit(_ path: String) -> Node {
        let b = text
        var n: Node = ["kind": "text_field", "id": path, "placeholder": placeholder, "text": b.wrappedValue,
                       "field": _Runtime.shared.registerField { b.wrappedValue = $0 }]
        if secure { n["secure"] = true }
        return n
    }
}

/// A `TextField` whose text the host draws as bullets.
public struct SecureField: View {
    let placeholder: String, text: Binding<String>
    public init(_ placeholder: String, text: Binding<String>) { self.placeholder = placeholder; self.text = text }
    public var body: some View { TextField(placeholder, text: text, secure: true) }
}

/// `Form` is an inset-grouped `List` on iOS.
public struct Form<Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let content: Content
    public init(@ViewBuilder content: () -> Content) { self.content = content() }
    func _emit(_ path: String) -> Node { List { content }._emit(path) }
}

/// Label leading, content trailing in the secondary style (a list row's trailing value).
public struct LabeledContent<Label: View, Content: View>: View {
    let label: Label, content: Content
    public init(@ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label) { self.content = content(); self.label = label() }
    public var body: some View {
        HStack {
            label
            Spacer()
            content.foregroundStyle(.secondary)
        }
    }
}
extension LabeledContent where Label == Text {
    public init(_ title: String, @ViewBuilder content: () -> Content) { self.init(content: content) { Text(title) } }
}
extension LabeledContent where Label == Text, Content == Text {
    public init(_ title: String, value: String) { self.init(title) { Text(value) } }
}

/// `Slider`: the host sets the value through the `field` id (a `text_input` carrying the number).
public struct Slider<Label: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let get: () -> Double, set: (Double) -> Void, lo: Double, hi: Double, step: Double?
    init<V: BinaryFloatingPoint>(_ value: Binding<V>, _ bounds: ClosedRange<V>, _ step: Double?) {
        get = { Double(value.wrappedValue) }; set = { value.wrappedValue = V($0) }
        lo = Double(bounds.lowerBound); hi = Double(bounds.upperBound); self.step = step
    }
    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V> = 0...1, @ViewBuilder label: () -> Label) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, nil)
    }
    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V>, step: V.Stride, @ViewBuilder label: () -> Label) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, Double(step))
    }
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared, (set, lo, hi, step) = (set, lo, hi, step)
        var n: Node = ["kind": "slider", "id": path, "value": get(), "min": lo, "max": hi, "disabled": rt.disabled]
        if let step { n["step"] = step }
        if !rt.disabled {
            n["field"] = rt.registerField { s in
                guard var v = Double(s) else { return }
                if let step, step > 0 { v = lo + ((v - lo) / step).rounded() * step }
                set(min(hi, max(lo, v)))
            }
        }
        return n
    }
}
extension Slider where Label == EmptyView {
    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V> = 0...1) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, nil)
    }
    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V>, step: V.Stride) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, Double(step))
    }
}

/// `ProgressView`: a linear bar when it has a value, else the (static) spinner.
public struct ProgressView<Label: View, CurrentValueLabel: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let fraction: Double?, label: Label?
    func _emit(_ path: String) -> Node {
        var n: Node = ["kind": "progress", "id": path, "value": fraction.map { min(1, max(0, $0)) } ?? NSNull()]
        if let label { n["label"] = _eval(label, path + "/0") }
        return n
    }
}
extension ProgressView where CurrentValueLabel == EmptyView {
    public init(@ViewBuilder label: () -> Label) { fraction = nil; self.label = label() }
    public init<V: BinaryFloatingPoint>(value: V?, total: V = 1.0, @ViewBuilder label: () -> Label) {
        fraction = value.map { Double($0) / Double(total) }; self.label = label()
    }
}
extension ProgressView where Label == EmptyView, CurrentValueLabel == EmptyView {
    public init() { fraction = nil; label = nil }
    public init<V: BinaryFloatingPoint>(value: V?, total: V = 1.0) { fraction = value.map { Double($0) / Double(total) }; label = nil }
}
extension ProgressView where Label == Text, CurrentValueLabel == EmptyView {
    public init(_ title: String) { self.init { Text(title) } }
    public init<V: BinaryFloatingPoint>(_ title: String, value: V?, total: V = 1.0) { self.init(value: value, total: total) { Text(title) } }
}

// MARK: - Picker

public protocol PickerStyle { var _styleName: String { get } }
public struct DefaultPickerStyle: PickerStyle { public var _styleName: String { "menu" } }
public struct MenuPickerStyle: PickerStyle { public var _styleName: String { "menu" } }
public struct SegmentedPickerStyle: PickerStyle { public var _styleName: String { "segmented" } }
extension PickerStyle where Self == DefaultPickerStyle { public static var automatic: DefaultPickerStyle { .init() } }
extension PickerStyle where Self == MenuPickerStyle { public static var menu: MenuPickerStyle { .init() } }
extension PickerStyle where Self == SegmentedPickerStyle { public static var segmented: SegmentedPickerStyle { .init() } }

struct _TagView<Content: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, tag: AnyHashable
    func _children(_ path: String) -> [Node] {
        let out = SwiftUI._children(content, path)
        for n in out { if let id = n["id"] as? String { _Runtime.shared.tags[id] = tag } }
        return out
    }
}

extension View {
    public func tag<V: Hashable>(_ tag: V) -> some View { _TagView(content: self, tag: AnyHashable(tag)) }
    public func pickerStyle<S: PickerStyle>(_ style: S) -> some View {
        _CtxView(content: self) { $0.pickerStyle = style._styleName }
    }
}

/// First text of an evaluated node (option labels).
func _plainText(_ n: Node) -> String? {
    if let t = n["text"] as? String { return t }
    for k in ["title", "label", "child"] { if let c = n[k] as? Node, let t = _plainText(c) { return t } }
    return (n["children"] as? [Node])?.lazy.compactMap(_plainText).first
}

final class _PickerState { var open = false }

public struct Picker<Label: View, SelectionValue: Hashable, Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let selection: Binding<SelectionValue>, content: Content, label: Label
    public init(selection: Binding<SelectionValue>, @ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label) {
        self.selection = selection; self.content = content(); self.label = label()
    }
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared, sel = selection, style = rt.pickerStyle
        let key = "\(path)#Picker"
        rt.touched.insert(key)
        let st = (rt.slots[key] as? _PickerState) ?? _PickerState()
        rt.slots[key] = st
        let opts: [(String, AnyHashable)] = SwiftUI._children(content, path + "/c").compactMap { n in
            guard let id = n["id"] as? String, let tag = rt.tags[id] else { return nil }
            return (_plainText(n) ?? "", tag)
        }
        let current = AnyHashable(sel.wrappedValue)
        let selected = opts.firstIndex { $0.1 == current }
        let pick = { (tag: AnyHashable) -> UInt64 in
            rt.disabled ? 0 : rt.registerAction { st.open = false; rt.dirty = true; if let v = tag.base as? SelectionValue { sel.wrappedValue = v } }
        }
        var n: Node = ["kind": "picker", "id": path, "style": style, "label": _eval(label, path + "/0"), "disabled": rt.disabled,
                       "options": opts.map { ["text": $0.0, "action": pick($0.1)] as Node }, "selected": selected ?? NSNull()]
        if style == "menu" {
            n["open"] = rt.disabled ? 0 : rt.registerAction { st.open.toggle(); rt.dirty = true }
            if st.open {
                // iOS 26 menu: a popover anchored to the row; a tap outside closes it.
                rt.layers.append(["kind": "menu", "anchor": path, "dismiss": rt.registerAction { st.open = false; rt.dirty = true },
                                  "items": opts.enumerated().map { i, o in ["text": o.0, "action": pick(o.1), "checked": i == selected] as Node }])
            }
        }
        return n
    }
}
extension Picker where Label == Text {
    public init(_ title: String, selection: Binding<SelectionValue>, @ViewBuilder content: () -> Content) {
        self.init(selection: selection, content: content) { Text(title) }
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
    /// Destination-style screens. With a bound path they sit on top of the path-driven screens (back pops them first).
    var stack: [_NavScreen] = []
    var titles: [Int: String] = [:]   // last evaluated title per depth, for the back label
    var destinations: [ObjectIdentifier: (Any, String) -> Node] = [:]   // kept across evaluations: hidden screens register nothing
    var ext: _NavPath?
    var depth: Int { (ext?.count() ?? 0) + stack.count }
    func push(_ s: _NavScreen) { stack.append(s); _Runtime.shared.dirty = true }
    func pushValue(_ v: Any) {
        // Like SwiftUI: a value nobody handles pushes nothing.
        guard destinations[ObjectIdentifier(type(of: v))] != nil else {
            #if !os(WASI)
            fputs("A NavigationLink is presenting a value of type \(type(of: v)) but there is no matching navigationDestination declaration visible from the location of the link.\n", stderr)
            #endif
            return
        }
        // ponytail: a value pushed from a destination screen goes on the screen stack, not into the bound path.
        if let ext, stack.isEmpty { ext.append(v) } else { stack.append(_NavScreen { [self] in build(v, $0) }) }
        _Runtime.shared.dirty = true
    }
    func screen(_ i: Int, _ path: String) -> Node {
        let n = ext?.count() ?? 0
        return i < n ? build(ext!.element(i), path) : stack[i - n].build(path)
    }
    func build(_ v: Any, _ path: String) -> Node {
        destinations[ObjectIdentifier(type(of: v))]?(v, path) ?? ["kind": "empty", "id": path]
    }
    func pop() {
        if !stack.isEmpty { stack.removeLast() } else if let ext, ext.count() > 0 { ext.removeLast() }
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

// MARK: - TabView

/// One tab: its content (evaluated only while selected) at `path`, plus what its tab bar item shows.
struct _TabEntry {
    var path: String, tag: AnyHashable?, badge: String?, title: String?, systemImage: String?
    var content: any View
}

/// Views that contribute tabs to a `TabView`: tuples, `Tab`, and the `.tabItem` / `.tag` / `.badge` modifiers.
protocol _TabSource { func _tabs(_ path: String) -> [_TabEntry] }

func _tabEntries(_ v: any View, _ path: String) -> [_TabEntry] {
    (v as? _TabSource)?._tabs(path) ?? [_TabEntry(path: path, tag: nil, badge: nil, title: nil, systemImage: nil, content: v)]
}

/// First `systemName` in an evaluated node (tab item icons).
func _firstSymbol(_ n: Node) -> String? {
    if let s = n["systemName"] as? String { return s }
    for k in ["icon", "label", "child", "title"] { if let c = n[k] as? Node, let s = _firstSymbol(c) { return s } }
    return (n["children"] as? [Node])?.lazy.compactMap(_firstSymbol).first
}

extension TupleView: _TabSource {
    func _tabs(_ path: String) -> [_TabEntry] {
        Mirror(reflecting: value).children.enumerated().flatMap { i, ch in
            (ch.value as? any View).map { _tabEntries($0, "\(path)/\(i)") } ?? []
        }
    }
}

extension _TagView: _TabSource {
    func _tabs(_ path: String) -> [_TabEntry] { _tabEntries(content, path).map { var e = $0; e.tag = e.tag ?? tag; return e } }
}

struct _TabItemView<Content: View, Item: View>: View, _Flattenable, _TabSource {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, item: Item
    func _children(_ path: String) -> [Node] { SwiftUI._children(content, path) }
    func _tabs(_ path: String) -> [_TabEntry] {
        let n = _eval(item, path + "/item")
        return _tabEntries(content, path).map { var e = $0; e.title = _plainText(n); e.systemImage = _firstSymbol(n); return e }
    }
}

struct _BadgeView<Content: View>: View, _Flattenable, _TabSource {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, badge: String?
    func _children(_ path: String) -> [Node] { SwiftUI._children(content, path) }   // ponytail: badges show on tab items only
    func _tabs(_ path: String) -> [_TabEntry] { _tabEntries(content, path).map { var e = $0; e.badge = badge; return e } }
}

extension View {
    public func tabItem<V: View>(@ViewBuilder _ label: () -> V) -> some View { _TabItemView(content: self, item: label()) }
    /// 0 hides the badge.
    public func badge(_ count: Int) -> some View { _BadgeView(content: self, badge: count == 0 ? nil : String(count)) }
    public func badge<S: StringProtocol>(_ label: S?) -> some View { _BadgeView(content: self, badge: label.map { String($0) }) }
}

/// iOS 18 `Tab`. A `View` here, so `TabView`'s `@ViewBuilder` takes it alongside `.tabItem` children.
public struct Tab<Content: View>: View, _TabSource {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let title: String, systemImage: String, value: AnyHashable?, content: Content
    public init(_ title: String, systemImage: String, @ViewBuilder content: () -> Content) {
        self.title = title; self.systemImage = systemImage; value = nil; self.content = content()
    }
    public init<V: Hashable>(_ title: String, systemImage: String, value: V, @ViewBuilder content: () -> Content) {
        self.title = title; self.systemImage = systemImage; self.value = AnyHashable(value); self.content = content()
    }
    func _tabs(_ path: String) -> [_TabEntry] { [_TabEntry(path: path, tag: value, badge: nil, title: title, systemImage: systemImage, content: content)] }
}

final class _TabState { var selected = 0 }

/// Only the selected tab is evaluated; the others' @State (keyed under their tab path) is kept alive.
public struct TabView<SelectionValue: Hashable, Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let selection: Binding<SelectionValue>?, content: Content
    public init(selection: Binding<SelectionValue>, @ViewBuilder content: () -> Content) { self.selection = selection; self.content = content() }

    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared, binding = selection
        let key = "\(path)#TabView"
        rt.touched.insert(key)
        let st = (rt.slots[key] as? _TabState) ?? _TabState()
        rt.slots[key] = st
        let tabs = _tabEntries(content, path + "/t")
        let sel = binding.flatMap { b in tabs.firstIndex { $0.tag == AnyHashable(b.wrappedValue) } } ?? min(st.selected, max(tabs.count - 1, 0))
        for (i, t) in tabs.enumerated() where i != sel {
            for k in rt.slots.keys where k.hasPrefix(t.path + "/") || k.hasPrefix(t.path + "#") { rt.touched.insert(k) }
        }
        let items: [Node] = tabs.enumerated().map { i, t in
            let action = rt.disabled ? 0 : rt.registerAction {
                if let binding, let v = t.tag?.base as? SelectionValue { binding.wrappedValue = v } else { st.selected = i; rt.dirty = true }
            }
            var n: Node = ["title": t.title ?? "", "action": action]
            if let s = t.systemImage { n["systemImage"] = s }
            if let b = t.badge { n["badge"] = b }
            return n
        }
        let content: Node = tabs.isEmpty ? ["kind": "empty", "id": path + "/t"] : _eval(tabs[sel].content, tabs[sel].path)
        return ["kind": "tabview", "id": path, "selected": sel, "tabs": items, "content": content]
    }
}
extension TabView where SelectionValue == Int {
    public init(@ViewBuilder content: () -> Content) { selection = nil; self.content = content() }
}
