#if os(WASI)
import WASILibc
#else
import Foundation
#endif

// Control styles, Menu, Link and Gauge (protocol section Q). Built-in styles are wire names; a custom style's
// `makeBody` is evaluated in place of the control, with the style cleared so its own controls draw normally.

/// A style configuration's view (`configuration.label`): the control's own view, evaluated where the style places it.
public struct _StyleLabel: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let make: (String) -> [Node]
    func _children(_ path: String) -> [Node] { make(path) }
}

func _styleLabel<V: View>(_ v: V) -> _StyleLabel { _StyleLabel { SwiftUI._children(v, $0) } }

/// A custom style's body, type-erased: builds the control's node at a path from its configuration.
typealias _StyleBody<C> = (C, String) -> Node

enum _ButtonStyleFn {
    case styled(_StyleBody<ButtonStyleConfiguration>)
    case primitive(_StyleBody<PrimitiveButtonStyleConfiguration>)
}
enum _NamedOr<C> {
    case name(String)
    case custom(_StyleBody<C>)
}

/// A built-in style's wire name, else the custom body.
func _named<C>(_ name: String?, _ body: @escaping _StyleBody<C>) -> _NamedOr<C> {
    if let name { return .name(name) }
    return .custom(body)
}

/// Style and environment context of the evaluation (saved and restored by `_CtxView`).
struct _Styles {
    var button: _ButtonStyleFn?
    var toggle: _NamedOr<ToggleStyleConfiguration>?
    var progress: _NamedOr<ProgressViewStyleConfiguration>?
    var gauge: _NamedOr<GaugeStyleConfiguration>?
    var menu: _StyleBody<MenuStyleConfiguration>?
    var datePicker: _NamedOr<DatePickerStyleConfiguration>?
    var labelsHidden = false
    /// Inside a `List` / `Form`: inline pickers become rows.
    var inList = false
}

/// Run `f` with one style cleared (a custom style's own body draws its controls unstyled).
func _without<T>(_ clear: (inout _Styles) -> Void, _ f: () -> T) -> T {
    let rt = _Runtime.shared, saved = rt.styles
    clear(&rt.styles)
    defer { rt.styles = saved }
    return f()
}

// MARK: - Button styles

public struct ButtonStyleConfiguration {
    public typealias Label = _StyleLabel
    public let role: ButtonRole?
    public let label: Label
    /// Always false: OneSim has no pressed state for custom styles.
    public let isPressed: Bool
}

public protocol ButtonStyle {
    associatedtype Body: View
    typealias Configuration = ButtonStyleConfiguration
    @ViewBuilder func makeBody(configuration: Configuration) -> Body
}

public struct PrimitiveButtonStyleConfiguration {
    public typealias Label = _StyleLabel
    public let role: ButtonRole?
    public let label: Label
    let action: () -> Void
    public func trigger() { action() }
}

public struct BorderlessButtonStyle: PrimitiveButtonStyle {
    public init() {}
    public var _styleName: String? { "automatic" }   // iOS draws it like the default style
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
extension PrimitiveButtonStyle where Self == BorderlessButtonStyle { public static var borderless: BorderlessButtonStyle { .init() } }

extension View {
    public func buttonStyle<S: ButtonStyle>(_ style: S) -> some View {
        _CtxView(content: self) { $0.styles.button = .styled { cfg, p in _eval(style.makeBody(configuration: cfg), p) } }
    }
}

// MARK: - Toggle styles

public struct ToggleStyleConfiguration {
    public typealias Label = _StyleLabel
    public let label: Label
    @Binding public var isOn: Bool
    public var isMixed: Bool
    init(label: Label, isOn: Binding<Bool>) { self.label = label; _isOn = isOn; isMixed = false }
}

public protocol ToggleStyle {
    associatedtype Body: View
    typealias Configuration = ToggleStyleConfiguration
    @ViewBuilder func makeBody(configuration: Configuration) -> Body
    var _styleName: String? { get }
}
extension ToggleStyle { public var _styleName: String? { nil } }

public struct DefaultToggleStyle: ToggleStyle {
    public init() {}
    public var _styleName: String? { "switch" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
public struct SwitchToggleStyle: ToggleStyle {
    public init() {}
    public var _styleName: String? { "switch" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
public struct ButtonToggleStyle: ToggleStyle {
    public init() {}
    public var _styleName: String? { "button" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
extension ToggleStyle where Self == DefaultToggleStyle { public static var automatic: DefaultToggleStyle { .init() } }
extension ToggleStyle where Self == SwitchToggleStyle { public static var `switch`: SwitchToggleStyle { .init() } }
extension ToggleStyle where Self == ButtonToggleStyle { public static var button: ButtonToggleStyle { .init() } }

extension View {
    public func toggleStyle<S: ToggleStyle>(_ style: S) -> some View {
        _CtxView(content: self) {
            $0.styles.toggle = _named(style._styleName) { cfg, p in _eval(style.makeBody(configuration: cfg), p) }
        }
    }
    /// Hides the labels of pickers and toggles (they still name the control for inspection).
    public func labelsHidden() -> some View { _CtxView(content: self) { $0.styles.labelsHidden = true } }
}

// MARK: - Progress view styles

public struct ProgressViewStyleConfiguration {
    public typealias Label = _StyleLabel
    public typealias CurrentValueLabel = _StyleLabel
    public let fractionCompleted: Double?
    public var label: Label?
    public var currentValueLabel: CurrentValueLabel?
}

public protocol ProgressViewStyle {
    associatedtype Body: View
    typealias Configuration = ProgressViewStyleConfiguration
    @ViewBuilder func makeBody(configuration: Configuration) -> Body
    var _styleName: String? { get }
}
extension ProgressViewStyle { public var _styleName: String? { nil } }

public struct DefaultProgressViewStyle: ProgressViewStyle {
    public init() {}
    public var _styleName: String? { "automatic" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
public struct LinearProgressViewStyle: ProgressViewStyle {
    public init() {}
    public var _styleName: String? { "linear" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
public struct CircularProgressViewStyle: ProgressViewStyle {
    public init() {}
    public var _styleName: String? { "circular" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
extension ProgressViewStyle where Self == DefaultProgressViewStyle { public static var automatic: DefaultProgressViewStyle { .init() } }
extension ProgressViewStyle where Self == LinearProgressViewStyle { public static var linear: LinearProgressViewStyle { .init() } }
extension ProgressViewStyle where Self == CircularProgressViewStyle { public static var circular: CircularProgressViewStyle { .init() } }

extension View {
    public func progressViewStyle<S: ProgressViewStyle>(_ style: S) -> some View {
        _CtxView(content: self) {
            $0.styles.progress = _named(style._styleName) { cfg, p in _eval(style.makeBody(configuration: cfg), p) }
        }
    }
}

// MARK: - Gauge

public struct GaugeStyleConfiguration {
    public typealias Label = _StyleLabel
    public typealias CurrentValueLabel = _StyleLabel
    public typealias MarkedValueLabel = _StyleLabel
    /// Fraction of the range, 0...1.
    public let value: Double
    public var label: Label
    public var currentValueLabel: CurrentValueLabel?
    public var minimumValueLabel: MarkedValueLabel?
    public var maximumValueLabel: MarkedValueLabel?
}

public protocol GaugeStyle {
    associatedtype Body: View
    typealias Configuration = GaugeStyleConfiguration
    @ViewBuilder func makeBody(configuration: Configuration) -> Body
    var _styleName: String? { get }
}
extension GaugeStyle { public var _styleName: String? { nil } }

public struct DefaultGaugeStyle: GaugeStyle {
    public init() {}
    public var _styleName: String? { "automatic" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
public struct LinearCapacityGaugeStyle: GaugeStyle {
    public init() {}
    public var _styleName: String? { "linearCapacity" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
public struct AccessoryLinearGaugeStyle: GaugeStyle {
    public init() {}
    public var _styleName: String? { "accessoryLinear" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
public struct AccessoryLinearCapacityGaugeStyle: GaugeStyle {
    public init() {}
    public var _styleName: String? { "accessoryLinearCapacity" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
public struct AccessoryCircularGaugeStyle: GaugeStyle {
    public init() {}
    public var _styleName: String? { "accessoryCircular" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
public struct AccessoryCircularCapacityGaugeStyle: GaugeStyle {
    public init() {}
    public var _styleName: String? { "accessoryCircularCapacity" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
extension GaugeStyle where Self == DefaultGaugeStyle { public static var automatic: DefaultGaugeStyle { .init() } }
extension GaugeStyle where Self == LinearCapacityGaugeStyle { public static var linearCapacity: LinearCapacityGaugeStyle { .init() } }
extension GaugeStyle where Self == AccessoryLinearGaugeStyle { public static var accessoryLinear: AccessoryLinearGaugeStyle { .init() } }
extension GaugeStyle where Self == AccessoryLinearCapacityGaugeStyle { public static var accessoryLinearCapacity: AccessoryLinearCapacityGaugeStyle { .init() } }
extension GaugeStyle where Self == AccessoryCircularGaugeStyle { public static var accessoryCircular: AccessoryCircularGaugeStyle { .init() } }
extension GaugeStyle where Self == AccessoryCircularCapacityGaugeStyle { public static var accessoryCircularCapacity: AccessoryCircularCapacityGaugeStyle { .init() } }

extension View {
    public func gaugeStyle<S: GaugeStyle>(_ style: S) -> some View {
        _CtxView(content: self) {
            $0.styles.gauge = _named(style._styleName) { cfg, p in _eval(style.makeBody(configuration: cfg), p) }
        }
    }
}

/// Evaluate an optional label; `nil` when it draws nothing (`EmptyView`).
func _optionalNode<V: View>(_ v: V?, _ path: String) -> Node? {
    guard let v else { return nil }
    let c = _children(v, path)
    if c.isEmpty { return nil }
    return c.count == 1 ? c[0] : ["kind": "vstack", "id": path, "spacing": 0.0, "alignment": "center", "children": c]
}

public struct Gauge<Label: View, CurrentValueLabel: View, BoundsLabel: View, MarkedValueLabels: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let fraction: Double, label: Label, current: CurrentValueLabel?, minLabel: BoundsLabel?, maxLabel: BoundsLabel?

    init<V: BinaryFloatingPoint>(_ value: V, _ bounds: ClosedRange<V>, _ label: Label, _ current: CurrentValueLabel?, _ lo: BoundsLabel?, _ hi: BoundsLabel?) {
        let span = Double(bounds.upperBound - bounds.lowerBound)
        fraction = span > 0 ? min(1, max(0, Double(value - bounds.lowerBound) / span)) : 0
        self.label = label; self.current = current; minLabel = lo; maxLabel = hi
    }
    public init<V: BinaryFloatingPoint>(value: V, in bounds: ClosedRange<V> = 0...1, @ViewBuilder label: () -> Label,
                                        @ViewBuilder currentValueLabel: () -> CurrentValueLabel,
                                        @ViewBuilder minimumValueLabel: () -> BoundsLabel,
                                        @ViewBuilder maximumValueLabel: () -> BoundsLabel) where MarkedValueLabels == EmptyView {
        self.init(value, bounds, label(), currentValueLabel(), minimumValueLabel(), maximumValueLabel())
    }

    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared
        if case .custom(let f)? = rt.styles.gauge {
            let cfg = GaugeStyleConfiguration(value: fraction, label: _styleLabel(label), currentValueLabel: current.map(_styleLabel),
                                              minimumValueLabel: minLabel.map(_styleLabel), maximumValueLabel: maxLabel.map(_styleLabel))
            return _without({ $0.gauge = nil }) { f(cfg, path) }
        }
        var style = "automatic"
        if case .name(let n)? = rt.styles.gauge { style = n }
        var n: Node = ["kind": "gauge", "id": path, "style": style, "value": fraction]
        for (k, v) in [("label", _optionalNode(label, path + "/l")), ("currentValueLabel", _optionalNode(current, path + "/c")),
                       ("minimumValueLabel", _optionalNode(minLabel, path + "/min")), ("maximumValueLabel", _optionalNode(maxLabel, path + "/max"))] {
            if let v { n[k] = v }
        }
        return n
    }
}
extension Gauge where CurrentValueLabel == EmptyView, BoundsLabel == EmptyView, MarkedValueLabels == EmptyView {
    public init<V: BinaryFloatingPoint>(value: V, in bounds: ClosedRange<V> = 0...1, @ViewBuilder label: () -> Label) {
        self.init(value, bounds, label(), nil, nil, nil)
    }
}
extension Gauge where BoundsLabel == EmptyView, MarkedValueLabels == EmptyView {
    public init<V: BinaryFloatingPoint>(value: V, in bounds: ClosedRange<V> = 0...1, @ViewBuilder label: () -> Label,
                                        @ViewBuilder currentValueLabel: () -> CurrentValueLabel) {
        self.init(value, bounds, label(), currentValueLabel(), nil, nil)
    }
}

// MARK: - Menu

public struct MenuStyleConfiguration {
    public typealias Label = _StyleLabel
    public typealias Content = _StyleLabel
    let emit: (String) -> Node
}

public protocol MenuStyle {
    associatedtype Body: View
    typealias Configuration = MenuStyleConfiguration
    @ViewBuilder func makeBody(configuration: Configuration) -> Body
    var _isBuiltIn: Bool { get }
}
extension MenuStyle { public var _isBuiltIn: Bool { false } }

public struct DefaultMenuStyle: MenuStyle {
    public init() {}
    public var _isBuiltIn: Bool { true }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
/// iOS draws `.button` menus like `.automatic`; the label follows a built-in `.buttonStyle`.
public struct ButtonMenuStyle: MenuStyle {
    public init() {}
    public var _isBuiltIn: Bool { true }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
public struct BorderlessButtonMenuStyle: MenuStyle {
    public init() {}
    public var _isBuiltIn: Bool { true }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
extension MenuStyle where Self == DefaultMenuStyle { public static var automatic: DefaultMenuStyle { .init() } }
extension MenuStyle where Self == ButtonMenuStyle { public static var button: ButtonMenuStyle { .init() } }
extension MenuStyle where Self == BorderlessButtonMenuStyle { public static var borderlessButton: BorderlessButtonMenuStyle { .init() } }

extension View {
    public func menuStyle<S: MenuStyle>(_ style: S) -> some View {
        _CtxView(content: self) { $0.styles.menu = style._isBuiltIn ? nil : { cfg, p in _eval(style.makeBody(configuration: cfg), p) } }
    }
}

final class _MenuState { var open = false }

/// Menu items from the content's evaluated nodes: buttons (icon from their label, red when destructive, dimmed
/// when disabled), toggles (checked when on) and a picker's options (the selected one checked, set off by
/// separators); a `Divider` or a `Section` boundary puts a separator above the next item.
func _menuItems(_ nodes: [Node], _ items: inout [Node], _ divider: inout Bool) {
    func add(_ it: Node) {
        var it = it
        if divider && !items.isEmpty { it["divider"] = true }
        divider = false
        items.append(it)
    }
    for n in nodes {
        switch n["kind"] as? String {
        case "divider": divider = true
        case "_section":
            divider = true
            _menuItems((n["rows"] as? [Node]) ?? [], &items, &divider)
            divider = true
        case "button":
            var it: Node = ["text": _plainText((n["label"] as? Node) ?? [:]) ?? "", "action": n["action"] ?? 0]
            if let l = n["label"] as? Node, let s = _firstSymbol(l) { it["systemImage"] = s }
            if n["role"] as? String == "destructive" { it["destructive"] = true }
            if n["disabled"] as? Bool == true { it["disabled"] = true }
            if let on = n["selected"] as? Bool { it["checked"] = on }   // a `.toggleStyle(.button)` toggle
            add(it)
        case "toggle":
            var it: Node = ["text": _plainText((n["label"] as? Node) ?? [:]) ?? "", "action": n["action"] ?? 0, "checked": n["isOn"] as? Bool ?? false]
            if n["disabled"] as? Bool == true { it["disabled"] = true }
            add(it)
        case "picker":
            divider = true
            let sel = n["selected"] as? Int
            for (i, o) in ((n["options"] as? [Node]) ?? []).enumerated() {
                var it: Node = ["text": o["text"] ?? "", "action": o["action"] ?? 0, "checked": i == sel]
                if n["disabled"] as? Bool == true { it["disabled"] = true }
                add(it)
            }
            divider = true
        default:
            if let c = n["children"] as? [Node] { _menuItems(c, &items, &divider) }
        }
    }
}

public struct Menu<Label: View, Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let label: Label?, content: Content?, primaryAction: (() -> Void)?
    let configuration: MenuStyleConfiguration?

    public init(@ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label) {
        self.content = content(); self.label = label(); primaryAction = nil; configuration = nil
    }
    /// A tap runs `primaryAction`; the menu itself opens on a long press on iOS, which OneSim does not offer.
    public init(@ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label, primaryAction: @escaping () -> Void) {
        self.content = content(); self.label = label(); self.primaryAction = primaryAction; configuration = nil
    }

    func _emit(_ path: String) -> Node {
        if let configuration { return configuration.emit(path) }
        let rt = _Runtime.shared
        if let f = rt.styles.menu {
            let cfg = MenuStyleConfiguration { p in self._menu(p) }
            return _without({ $0.menu = nil }) { f(cfg, path) }
        }
        return _menu(path)
    }

    func _menu(_ path: String) -> Node {
        let rt = _Runtime.shared
        let key = "\(path)#Menu"
        rt.touched.insert(key)
        let st = (rt.slots[key] as? _MenuState) ?? _MenuState()
        rt.slots[key] = st
        var items: [Node] = [], divider = false
        let nodes = _without({ $0.button = nil }) { content.map { SwiftUI._children($0, path + "/c") } ?? [] }
        _menuItems(nodes, &items, &divider)
        // An item runs its button's action and closes the menu; disabled items do nothing.
        for i in items.indices where items[i]["disabled"] == nil {
            let inner = rt.actions[(items[i]["action"] as? UInt64) ?? 0]
            items[i]["action"] = rt.registerAction { st.open = false; rt.dirty = true; inner?() }
        }
        let toggle = { st.open.toggle(); rt.dirty = true }
        var n: Node = ["kind": "button", "id": path, "style": rt.buttonStyle, "disabled": rt.disabled,
                       "action": rt.disabled ? 0 : rt.registerAction(primaryAction ?? toggle),
                       "label": label.map { _eval($0, path + "/0") } ?? ["kind": "empty", "id": path + "/0"]]
        if rt.disabled { st.open = false }
        if st.open {
            rt.layers.append(["kind": "menu", "anchor": path, "centered": true, "items": items,
                              "dismiss": rt.registerAction { st.open = false; rt.dirty = true }])
            n["value"] = "open"
        }
        return n
    }
}
extension Menu where Label == Text {
    public init(_ title: String, @ViewBuilder content: () -> Content) { self.init(content: content) { Text(title) } }
}
extension Menu where Label == SwiftUI.Label<Text, Image> {
    public init(_ title: String, systemImage: String, @ViewBuilder content: () -> Content) {
        self.init(content: content) { SwiftUI.Label(title, systemImage: systemImage) }
    }
}
extension Menu where Label == MenuStyleConfiguration.Label, Content == MenuStyleConfiguration.Content {
    /// The menu a custom `MenuStyle` wraps.
    public init(_ configuration: MenuStyleConfiguration) {
        label = nil; content = nil; primaryAction = nil; self.configuration = configuration
    }
}

// MARK: - Link and openURL

public struct Link<Label: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let destination: URL, label: Label
    public init(destination: URL, @ViewBuilder label: () -> Label) { self.destination = destination; self.label = label() }
    func _emit(_ path: String) -> Node {
        #if os(WASI)
        return Button(action: {}) { label }._emit(path)   // no openURL on the web build
        #else
        let open = EnvironmentValues().openURL, url = destination
        return Button(action: { open(url) }) { label }._emit(path)
        #endif
    }
}
extension Link where Label == Text {
    public init(_ title: String, destination: URL) { self.init(destination: destination) { Text(title) } }
}
