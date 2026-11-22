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

public struct ForEach<Data: RandomAccessCollection, ID: Hashable, Content: View>: DynamicViewContent, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    public let data: Data
    let id: KeyPath<Data.Element, ID>, content: (Data.Element) -> Content
    public init(_ data: Data, id: KeyPath<Data.Element, ID>, @ViewBuilder content: @escaping (Data.Element) -> Content) {
        self.data = data; self.id = id; self.content = content
    }
    // Element id goes into the structural path so @State follows items, not positions.
    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared, actions = rt.rowActions, count = data.count
        // Delete and move handlers belong to this ForEach's rows, not to a nested ForEach's.
        rt.rowActions = _RowActions()
        defer { rt.rowActions = actions }
        return data.enumerated().flatMap { i, e -> [Node] in
            let nodes = _selectionRow(content(e), AnyHashable(e[keyPath: id]), "\(path)/\(e[keyPath: id])")
            rt.rowActions = actions
            _registerElementRows(nodes, index: i, count: count)
            rt.rowActions = _RowActions()
            return nodes
        }
    }
}
extension ForEach where ID == Data.Element.ID, Data.Element: Identifiable {
    public init(_ data: Data, @ViewBuilder content: @escaping (Data.Element) -> Content) { self.init(data, id: \.id, content: content) }
}
extension ForEach where Data == Range<Int>, ID == Int {
    public init(_ data: Range<Int>, @ViewBuilder content: @escaping (Int) -> Content) { self.init(data, id: \.self, content: content) }
}

/// Groups views, or scenes (`Group { WindowGroup ... }`), with no effect on layout.
public struct Group<Content> {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let content: Content
}
extension Group: View, _Flattenable, _ModifierTransparent where Content: View {
    public init(@ViewBuilder content: () -> Content) { self.content = content() }
    func _children(_ path: String) -> [Node] { SwiftUI._children(content, path) }
}

// MARK: - Controls

/// `Slider`: the host sets the value through the `field` id (a `text_input` carrying the number). Value labels
/// sit beside it in an `hstack`; `onEditingChanged` brackets each change (true, set, false).
public struct Slider<Label: View, ValueLabel: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let get: () -> Double, set: (Double) -> Void, lo: Double, hi: Double, step: Double?
    var minLabel: ValueLabel?, maxLabel: ValueLabel?, editing: (Bool) -> Void = { _ in }
    init<V: BinaryFloatingPoint>(_ value: Binding<V>, _ bounds: ClosedRange<V>, _ step: Double?, _ editing: @escaping (Bool) -> Void) {
        get = { Double(value.wrappedValue) }; set = { value.wrappedValue = V($0) }
        lo = Double(bounds.lowerBound); hi = Double(bounds.upperBound); self.step = step; self.editing = editing
    }
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared, (set, lo, hi, step, editing) = (set, lo, hi, step, editing)
        var n: Node = ["kind": "slider", "id": path, "value": get(), "min": lo, "max": hi, "disabled": rt.disabled]
        if let step { n["step"] = step }
        if !rt.disabled {
            n["field"] = rt.registerField { s in
                guard var v = Double(s) else { return }
                if let step, step > 0 { v = lo + ((v - lo) / step).rounded() * step }
                editing(true)
                set(min(hi, max(lo, v)))
                editing(false)
            }
        }
        let minNode = _optionalNode(minLabel, path + "/min"), maxNode = _optionalNode(maxLabel, path + "/max")
        if minNode == nil && maxNode == nil { return n }
        n["id"] = path + "/s"
        return ["kind": "hstack", "id": path, "spacing": _sliderLabelGap, "alignment": "center", "children": [minNode, n, maxNode].compactMap { $0 }]
    }
}
/// Gap between a slider and its value labels (iOS 27.2, swiftui-controls indicators).
let _sliderLabelGap = 11.0
extension Slider where ValueLabel == EmptyView {
    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V> = 0...1, @ViewBuilder label: () -> Label,
                                        onEditingChanged: @escaping (Bool) -> Void = { _ in }) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, nil, onEditingChanged)
    }
    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V>, step: V.Stride, @ViewBuilder label: () -> Label,
                                        onEditingChanged: @escaping (Bool) -> Void = { _ in }) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, Double(step), onEditingChanged)
    }
}
extension Slider {
    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V> = 0...1, @ViewBuilder label: () -> Label,
                                        @ViewBuilder minimumValueLabel: () -> ValueLabel, @ViewBuilder maximumValueLabel: () -> ValueLabel,
                                        onEditingChanged: @escaping (Bool) -> Void = { _ in }) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, nil, onEditingChanged)
        minLabel = minimumValueLabel(); maxLabel = maximumValueLabel()
    }
    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V>, step: V.Stride, @ViewBuilder label: () -> Label,
                                        @ViewBuilder minimumValueLabel: () -> ValueLabel, @ViewBuilder maximumValueLabel: () -> ValueLabel,
                                        onEditingChanged: @escaping (Bool) -> Void = { _ in }) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, Double(step), onEditingChanged)
        minLabel = minimumValueLabel(); maxLabel = maximumValueLabel()
    }
}
extension Slider where Label == EmptyView, ValueLabel == EmptyView {
    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V> = 0...1,
                                        onEditingChanged: @escaping (Bool) -> Void = { _ in }) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, nil, onEditingChanged)
    }
    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V>, step: V.Stride,
                                        onEditingChanged: @escaping (Bool) -> Void = { _ in }) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, Double(step), onEditingChanged)
    }
}

/// `ProgressView`: a linear bar when it has a value, else the (static) spinner. `.circular` is always the spinner
/// on iOS; a custom `ProgressViewStyle` replaces the view.
public struct ProgressView<Label: View, CurrentValueLabel: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let fraction: Double?, label: Label?
    var current: CurrentValueLabel? = nil
    var unstyled = false   // `ProgressView(configuration)`: the default look inside a custom style
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared
        var circular = false
        switch rt.styles.progress {
        case .custom(let f)? where !unstyled:
            let cfg = ProgressViewStyleConfiguration(fractionCompleted: fraction, label: label.map(_styleLabel), currentValueLabel: current.map(_styleLabel))
            return _without({ $0.progress = nil }) { f(cfg, path) }
        case .name(let n)?: circular = n == "circular"
        default: break
        }
        let value = circular ? nil : fraction.map { min(1, max(0, $0)) }
        var n: Node = ["kind": "progress", "id": path, "value": value ?? NSNull()]
        if let l = _optionalNode(label, path + "/0") { n["label"] = l }
        if value != nil, let c = _optionalNode(current, path + "/1") { n["currentValueLabel"] = c }
        return n
    }
}
extension ProgressView {
    public init<V: BinaryFloatingPoint>(value: V?, total: V = 1.0, @ViewBuilder label: () -> Label, @ViewBuilder currentValueLabel: () -> CurrentValueLabel) {
        fraction = value.map { Double($0) / Double(total) }; self.label = label(); current = currentValueLabel()
    }
}
extension ProgressView where Label == ProgressViewStyleConfiguration.Label, CurrentValueLabel == ProgressViewStyleConfiguration.CurrentValueLabel {
    /// The default look, for use inside a custom style's `makeBody`.
    public init(_ configuration: ProgressViewStyleConfiguration) {
        fraction = configuration.fractionCompleted; label = configuration.label; current = configuration.currentValueLabel; unstyled = true
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
public struct DefaultPickerStyle: PickerStyle { public init() {}; public var _styleName: String { "menu" } }
public struct MenuPickerStyle: PickerStyle { public init() {}; public var _styleName: String { "menu" } }
public struct SegmentedPickerStyle: PickerStyle { public init() {}; public var _styleName: String { "segmented" } }
public struct InlinePickerStyle: PickerStyle { public init() {}; public var _styleName: String { "inline" } }
public struct WheelPickerStyle: PickerStyle { public init() {}; public var _styleName: String { "wheel" } }
public struct NavigationLinkPickerStyle: PickerStyle { public init() {}; public var _styleName: String { "navigationLink" } }
/// Outside a menu iOS draws a palette like a segmented control.
public struct PalettePickerStyle: PickerStyle { public init() {}; public var _styleName: String { "segmented" } }
extension PickerStyle where Self == DefaultPickerStyle { public static var automatic: DefaultPickerStyle { .init() } }
extension PickerStyle where Self == MenuPickerStyle { public static var menu: MenuPickerStyle { .init() } }
extension PickerStyle where Self == SegmentedPickerStyle { public static var segmented: SegmentedPickerStyle { .init() } }
extension PickerStyle where Self == InlinePickerStyle { public static var inline: InlinePickerStyle { .init() } }
extension PickerStyle where Self == WheelPickerStyle { public static var wheel: WheelPickerStyle { .init() } }
extension PickerStyle where Self == NavigationLinkPickerStyle { public static var navigationLink: NavigationLinkPickerStyle { .init() } }
extension PickerStyle where Self == PalettePickerStyle { public static var palette: PalettePickerStyle { .init() } }

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

/// A list row choosing one picker option: its text, and an accent checkmark at the trailing edge when selected.
func _pickerRow(_ path: String, _ text: String, _ checked: Bool, _ action: UInt64, _ disabled: Bool) -> Node {
    var kids: [Node] = [["kind": "text", "id": path + "/t", "text": text], ["kind": "spacer", "id": path + "/s", "minLength": NSNull()]]
    if checked {
        kids.append(["kind": "image", "id": path + "/c", "systemName": "checkmark", "resizable": false,
                     "modifiers": [["foregroundColor": ["semantic": "accent"] as Node], ["fontWeight": "semibold"]] as [Node]])
    }
    return ["kind": "button", "id": path, "style": "plain", "disabled": disabled, "action": action, "value": checked ? "checked" : NSNull(),
            "label": ["kind": "hstack", "id": path + "/h", "spacing": NSNull(), "alignment": "center", "children": kids] as Node]
}

/// Styles: menu (also `.automatic`), segmented and palette, wheel, inline (rows with a checkmark in a list,
/// a wheel elsewhere), and navigationLink (a row that pushes the options as a list).
public struct Picker<Label: View, SelectionValue: Hashable, Content: View>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let selection: Binding<SelectionValue>, content: Content, label: Label
    public init(selection: Binding<SelectionValue>, @ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label) {
        self.selection = selection; self.content = content(); self.label = label()
    }
    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared, sel = selection, disabled = rt.disabled
        var style = rt.pickerStyle
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
            disabled ? 0 : rt.registerAction { st.open = false; rt.dirty = true; if let v = tag.base as? SelectionValue { sel.wrappedValue = v } }
        }
        let labelNode = rt.styles.labelsHidden ? ["kind": "empty", "id": path + "/0"] : _eval(label, path + "/0")
        if style == "inline" {
            guard rt.styles.inList else { return [_node(path, "wheel", labelNode, opts, selected, pick, st)] }
            return opts.enumerated().map { i, o in _pickerRow("\(path)/r\(i)", o.0, i == selected, pick(o.1), disabled) }
        }
        if style == "navigationLink", let nav = rt.nav {
            let title = _plainText(_eval(label, path + "/0")) ?? ""
            let value: Node = ["kind": "text", "id": path + "/v", "text": selected.map { opts[$0].0 } ?? "",
                               "modifiers": [["foregroundColor": ["semantic": "secondary"] as Node]] as [Node]]
            // The pushed list reads the selection when it is built; choosing an option pops back.
            let screen = _NavScreen { p in
                let cur = AnyHashable(sel.wrappedValue)
                let rows = opts.enumerated().map { i, o in
                    _pickerRow("\(p)/r\(i)", o.0, o.1 == cur, disabled ? 0 : rt.registerAction {
                        if let v = o.1.base as? SelectionValue { sel.wrappedValue = v }
                        nav.pop()
                    }, disabled)
                }
                return ["kind": "list", "id": p, "style": "insetGrouped", "sections": [["rows": rows] as Node],
                        "modifiers": [["navigationTitle": title], ["navigationBarTitleDisplayMode": "inline"]] as [Node]]
            }
            return [["kind": "button", "id": path, "style": "navigationLink", "disabled": disabled,
                     "action": disabled ? 0 : rt.registerAction { nav.push(screen) },
                     "label": ["kind": "hstack", "id": path + "/h", "spacing": NSNull(), "alignment": "center",
                               "children": [labelNode, ["kind": "spacer", "id": path + "/s", "minLength": NSNull()] as Node, value]] as Node]]
        }
        if style == "navigationLink" { style = "menu" }   // no NavigationStack to push onto
        return [_node(path, style, labelNode, opts, selected, pick, st)]
    }

    func _node(_ path: String, _ style: String, _ label: Node, _ opts: [(String, AnyHashable)], _ selected: Int?,
               _ pick: (AnyHashable) -> UInt64, _ st: _PickerState) -> Node {
        let rt = _Runtime.shared
        var n: Node = ["kind": "picker", "id": path, "style": style, "label": label, "disabled": rt.disabled,
                       "options": opts.map { ["text": $0.0, "action": pick($0.1)] as Node }, "selected": selected ?? NSNull()]
        if style == "menu" {
            if !rt.pageIsActive { st.open = false }
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

/// Styles: `.switch` / `.automatic` (a switch), `.button` (a button in an accent-tinted capsule while on), or a
/// custom `ToggleStyle`.
public struct Toggle<Label: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let isOn: Binding<Bool>, label: Label
    public init(isOn: Binding<Bool>, @ViewBuilder label: () -> Label) { self.isOn = isOn; self.label = label() }
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared, b = isOn
        let action = rt.disabled ? 0 : rt.registerAction { b.wrappedValue.toggle() }
        switch rt.styles.toggle {
        case .custom(let f)?:
            let cfg = ToggleStyleConfiguration(label: _styleLabel(label), isOn: b)
            return _without({ $0.toggle = nil }) { f(cfg, path) }
        case .name("button")?:
            return ["kind": "button", "id": path, "style": "automatic", "selected": b.wrappedValue, "disabled": rt.disabled,
                    "action": action, "label": _eval(label, path + "/0")]
        default:
            let l = rt.styles.labelsHidden ? ["kind": "empty", "id": path + "/0"] : _eval(label, path + "/0")
            return ["kind": "toggle", "id": path, "isOn": b.wrappedValue, "disabled": rt.disabled, "action": action, "label": l]
        }
    }
}
extension Toggle where Label == Text {
    public init(_ title: String, isOn: Binding<Bool>) { self.init(isOn: isOn) { Text(title) } }
}
extension Toggle where Label == ToggleStyleConfiguration.Label {
    /// The default switch, for use inside a custom style's `makeBody`.
    public init(_ configuration: ToggleStyleConfiguration) { self.init(isOn: configuration.$isOn) { configuration.label } }
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
    static func slotKey(_ path: String) -> String { path + "#NavigationStack" }
    /// Destination-style screens. With a bound path they sit on top of the path-driven screens (back pops them first).
    var stack: [_NavScreen] = []
    var titles: [Int: String] = [:]   // last evaluated title per depth, for the back label
    var destinations: [ObjectIdentifier: (Any, String) -> Node] = [:]   // kept across evaluations: hidden screens register nothing
    var ext: _NavPath?
    var rootBack: (() -> Void)?
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
        if !stack.isEmpty { stack.removeLast() } else if let ext, ext.count() > 0 { ext.removeLast() } else { rootBack?() }
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
        _navigation(path, externalPath: self.path) { _eval(root, $0) }
    }
}

func _navigation(_ path: String, columnPath: String? = nil, externalPath: _NavPath? = nil, rootBack: (() -> Void)? = nil,
                 backLabel: String = "Back", root: (String) -> Node) -> Node {
    let rt = _Runtime.shared
    let screenPath = columnPath ?? path
    let key = _NavState.slotKey(screenPath)
    rt.touched.insert(key)
    let st = (rt.slots[key] as? _NavState) ?? _NavState()
    rt.slots[key] = st
    st.ext = externalPath
    st.rootBack = rootBack
    let depth = st.depth
    // Screens below the top are not evaluated; keep their @State alive.
    for d in 0..<depth {
        let prefix = d == 0 ? "\(screenPath)/0" : "\(screenPath)/s\(d)"
        for k in rt.slots.keys where k.hasPrefix(prefix + "/") || k.hasPrefix(prefix + "#") { rt.touched.insert(k) }
    }
    let saved = (rt.nav, rt.disabled, rt.buttonStyle, rt.envLocal, rt.styles.inList)
    rt.nav = st; rt.disabled = false; rt.buttonStyle = "automatic"; rt.envLocal = []; rt.styles.inList = false
    defer { (rt.nav, rt.disabled, rt.buttonStyle, rt.envLocal, rt.styles.inList) = saved }
    let preferences = rt.navigationPreferences
    rt.navigationPreferences = [:]
    defer { rt.navigationPreferences = preferences }
    // Each screen has its own edit mode (EditButton, List editing), unless `\.editMode` is set with `.environment`
    // around the stack, which reaches every screen.
    let screen = depth == 0 ? "\(screenPath)/0" : "\(screenPath)/s\(depth)"
    let k = ObjectIdentifier(_EditModeKey.self), outer = rt.envObjects[k]
    if !saved.3.contains(k) { rt.envObjects[k] = nil }
    var content = _withEditMode(screen) { depth == 0 ? root(screen) : st.screen(depth - 1, screen) }
    rt.envObjects[k] = outer

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
    if depth == 0, rootBack != nil {
        nav["back"] = ["action": rt.registerAction { st.pop() }, "label": backLabel] as Node
    }
    rt.emittedNavigationStates[path] = st
    return _applyNavigationPreferences(rt.navigationPreferences ?? [:], to: nav)
}

struct _DestinationView<Content: View, T: Hashable, D: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, destination: (T) -> D
    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared, env = _localEnv(), dest = destination
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
        let rt = _Runtime.shared, nav = rt.nav, env = _localEnv()
        let action: () -> Void
        if let push { action = { if let nav { push(nav) } } }
        else { let d = destination!; action = { nav?.push(_NavScreen { p in _withEnv(env) { _eval(d, p) } }) } }
        // A list row with a chevron in a List / Form; elsewhere an ordinary (accent) button.
        return ["kind": "button", "id": path, "style": rt.styles.inList ? "navigationLink" : rt.buttonStyle, "disabled": rt.disabled,
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

/// One tab's content and tab bar item, keyed by its structural path.
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

extension EmptyView: _TabSource {
    func _tabs(_ path: String) -> [_TabEntry] { [] }
}

extension Optional: _TabSource where Wrapped: View {
    func _tabs(_ path: String) -> [_TabEntry] { map { _tabEntries($0, path) } ?? [] }
}

extension _ConditionalContent: _TabSource {
    func _tabs(_ path: String) -> [_TabEntry] {
        switch storage {
        case .first(let v): return _tabEntries(v, path + "/true")
        case .second(let v): return _tabEntries(v, path + "/false")
        }
    }
}

extension Group: _TabSource where Content: View {
    func _tabs(_ path: String) -> [_TabEntry] { _tabEntries(content, path) }
}

extension ForEach: _TabSource {
    func _tabs(_ path: String) -> [_TabEntry] {
        data.flatMap { element in
            let value = element[keyPath: id]
            return _tabEntries(content(element), "\(path)/\(value)").map { entry in
                var entry = entry
                entry.tag = entry.tag ?? AnyHashable(value)
                return entry
            }
        }
    }
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
    public func badge(_ label: Text?) -> some View { _BadgeView(content: self, badge: label?.runs.map(\.text).joined()) }
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

final class _TabState { var selectedPath: String? }

/// Bar styles evaluate the selected tab and retain the other tabs' state. Page style evaluates all pages.
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
        let selectedTag = binding.map { AnyHashable($0.wrappedValue) }
        let sel = tabs.firstIndex { selectedTag != nil && $0.tag == selectedTag }
            ?? tabs.firstIndex { $0.path == st.selectedPath } ?? 0
        let actions = tabs.map { tab in
            rt.disabled ? 0 : rt.registerAction {
                st.selectedPath = tab.path
                if let binding, let value = tab.tag?.base as? SelectionValue { binding.wrappedValue = value }
                else { rt.dirty = true }
            }
        }
        if let style = rt.pageTabStyle {
            let pages: [Node] = zip(tabs, actions).enumerated().map { i, page in
                let (tab, action) = page
                let wasActive = rt.pageIsActive
                rt.pageIsActive = wasActive && i == sel
                defer { rt.pageIsActive = wasActive }
                let content = _eval(tab.content, tab.path)
                var page: Node = ["content": content]
                if action != 0 { page["action"] = action }
                return page
            }
            return ["kind": "pagetabview", "id": path, "selected": sel, "pages": pages,
                    "indexDisplay": style.indexDisplayMode.name, "indexBackground": rt.pageIndexStyle.backgroundDisplayMode.name]
        }
        let items: [Node] = zip(tabs, actions).map { tab, action in
            var item: Node = ["title": tab.title ?? "", "action": action]
            item["systemImage"] = tab.systemImage
            item["badge"] = tab.badge
            return item
        }
        for (i, t) in tabs.enumerated() where i != sel {
            for k in rt.slots.keys where k.hasPrefix(t.path + "/") || k.hasPrefix(t.path + "#") { rt.touched.insert(k) }
        }
        let content: Node = tabs.isEmpty ? ["kind": "empty", "id": path + "/t"] : _eval(tabs[sel].content, tabs[sel].path)
        return ["kind": "tabview", "id": path, "selected": sel, "tabs": items, "content": content]
    }
}
extension TabView where SelectionValue == Int {
    public init(@ViewBuilder content: () -> Content) { selection = nil; self.content = content() }
}

// MARK: - TabView and index view styles

public protocol TabViewStyle {}
public struct DefaultTabViewStyle: TabViewStyle {
    public init() {}
}
public struct TabBarOnlyTabViewStyle: TabViewStyle {
    public init() {}
}
public struct SidebarAdaptableTabViewStyle: TabViewStyle {
    public init() {}
}
public struct PageTabViewStyle: TabViewStyle {
    public struct IndexDisplayMode: Sendable {
        let name: String
        public static let automatic = IndexDisplayMode(name: "automatic")
        public static let always = IndexDisplayMode(name: "always")
        public static let never = IndexDisplayMode(name: "never")
    }
    let indexDisplayMode: IndexDisplayMode
    public init(indexDisplayMode: IndexDisplayMode = .automatic) { self.indexDisplayMode = indexDisplayMode }
}
extension TabViewStyle where Self == DefaultTabViewStyle { public static var automatic: DefaultTabViewStyle { .init() } }
extension TabViewStyle where Self == TabBarOnlyTabViewStyle { public static var tabBarOnly: TabBarOnlyTabViewStyle { .init() } }
extension TabViewStyle where Self == SidebarAdaptableTabViewStyle { public static var sidebarAdaptable: SidebarAdaptableTabViewStyle { .init() } }
extension TabViewStyle where Self == PageTabViewStyle {
    public static var page: PageTabViewStyle { .init() }
    public static func page(indexDisplayMode: PageTabViewStyle.IndexDisplayMode) -> PageTabViewStyle { .init(indexDisplayMode: indexDisplayMode) }
}

public protocol IndexViewStyle {}
public struct PageIndexViewStyle: IndexViewStyle {
    public struct BackgroundDisplayMode: Sendable {
        let name: String
        public static let automatic = BackgroundDisplayMode(name: "automatic")
        public static let always = BackgroundDisplayMode(name: "always")
        public static let interactive = BackgroundDisplayMode(name: "interactive")
        public static let never = BackgroundDisplayMode(name: "never")
    }
    let backgroundDisplayMode: BackgroundDisplayMode
    public init(backgroundDisplayMode: BackgroundDisplayMode = .automatic) { self.backgroundDisplayMode = backgroundDisplayMode }
}
extension IndexViewStyle where Self == PageIndexViewStyle {
    public static var page: PageIndexViewStyle { .init() }
    public static func page(backgroundDisplayMode: PageIndexViewStyle.BackgroundDisplayMode) -> PageIndexViewStyle { .init(backgroundDisplayMode: backgroundDisplayMode) }
}

extension View {
    public func tabViewStyle<S: TabViewStyle>(_ style: S) -> some View {
        _CtxView(content: self) { $0.pageTabStyle = style as? PageTabViewStyle }
    }
    public func indexViewStyle<S: IndexViewStyle>(_ style: S) -> some View {
        _CtxView(content: self) { $0.pageIndexStyle = style as? PageIndexViewStyle ?? .init() }
    }
}
