#if !os(WASI)
import Foundation
#endif

public struct Text: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    /// One styled run: `attrs` holds the wire keys `font` / `fontWeight` / `foregroundColor` set on it.
    struct Run { var text: String; var attrs: Node }
    var runs: [Run]
    public init(_ content: String) { runs = [Run(text: content, attrs: [:])] }
    public init(verbatim content: String) { self.init(content) }

    /// `Text + Text`: keeps each side's own styling.
    public static func + (l: Text, r: Text) -> Text { var t = l; t.runs += r.runs; return t }

    // Text-returning modifiers (so the result can still be concatenated). On a concatenation an
    // explicit inner style wins over the outer one, as in SwiftUI.
    func _with(_ key: String, _ value: Any) -> Text {
        var t = self
        for i in t.runs.indices where runs.count == 1 || t.runs[i].attrs[key] == nil { t.runs[i].attrs[key] = value }
        return t
    }
    public func bold() -> Text { _with("fontWeight", "bold") }
    public func fontWeight(_ weight: Font.Weight?) -> Text { _with("fontWeight", weight?.name ?? "regular") }
    public func font(_ font: Font?) -> Text { _with("font", font?.json ?? ["style": "body"] as Node) }
    public func foregroundColor(_ color: Color?) -> Text { _with("foregroundColor", color?.json ?? ["semantic": "primary"] as Node) }
    public func foregroundStyle<S: ShapeStyle>(_ style: S) -> Text { _with("foregroundColor", style._json) }
    public func italic() -> Text { _with("italic", true) }
    public func underline(_ active: Bool = true) -> Text { _with("underline", active) }
    public func strikethrough(_ active: Bool = true) -> Text { _with("strikethrough", active) }

    func _emit(_ path: String) -> Node {
        var n: Node = ["kind": "text", "id": path, "text": runs.map { $0.text }.joined()]
        // Italic and underline exist only as run fields; everything else of a lone run is plain modifiers.
        if runs.count == 1, runs[0].attrs["italic"] == nil, runs[0].attrs["underline"] == nil {
            let a = runs[0].attrs
            let mods = a.keys.sorted().map { [$0: a[$0]!] as Node }
            if !mods.isEmpty { n["modifiers"] = mods }
        } else {
            n["runs"] = runs.map { r -> Node in var j = r.attrs; j["text"] = r.text; return j }
        }
        return n
    }
}

public struct Spacer: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let minLength: CGFloat?
    public init(minLength: CGFloat? = nil) { self.minLength = minLength }
    func _emit(_ path: String) -> Node { ["kind": "spacer", "id": path, "minLength": minLength.map { _num($0) } ?? NSNull()] }
}


public struct ButtonRole: Equatable {
    let name: String
    public static let cancel = ButtonRole(name: "cancel")
    public static let destructive = ButtonRole(name: "destructive")
}

public struct Button<Label: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let action: () -> Void
    let label: Label
    let role: ButtonRole?
    public init(role: ButtonRole? = nil, action: @escaping () -> Void, @ViewBuilder label: () -> Label) { self.role = role; self.action = action; self.label = label() }
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared
        // Buttons in an alert's actions also dismiss it (after running their own action).
        let act = rt.autoDismiss.map { d in { action(); d() } } ?? action
        if let custom = rt.styles.button {
            let lbl = _styleLabel(label), disabled = rt.disabled
            return _without({ $0.button = nil }) {
                switch custom {
                case .styled(let f):
                    // A `ButtonStyle` draws the label; the button stays a plain button around it.
                    var n: Node = ["kind": "button", "id": path, "action": disabled ? 0 : rt.registerAction(act), "style": "plain",
                                   "disabled": disabled, "label": f(ButtonStyleConfiguration(role: role, label: lbl, isPressed: false), path + "/0")]
                    if let role { n["role"] = role.name }
                    return n
                case .primitive(let f):
                    // A `PrimitiveButtonStyle` replaces the button; `trigger()` runs its action.
                    return f(PrimitiveButtonStyleConfiguration(role: role, label: lbl, action: disabled ? {} : act), path)
                }
            }
        }
        var n: Node = ["kind": "button", "id": path, "action": rt.disabled ? 0 : rt.registerAction(act), "label": _eval(label, path + "/0"),
                       "style": rt.buttonStyle, "disabled": rt.disabled]
        if let role { n["role"] = role.name }
        return n
    }
}
extension Button where Label == Text {
    public init(_ title: String, role: ButtonRole? = nil, action: @escaping () -> Void) { self.init(role: role, action: action) { Text(title) } }
}
extension Button where Label == SwiftUI.Label<Text, Image> {
    public init(_ title: String, systemImage: String, role: ButtonRole? = nil, action: @escaping () -> Void) {
        self.init(role: role, action: action) { SwiftUI.Label(title, systemImage: systemImage) }
    }
}

public struct VStack<Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let alignment: HorizontalAlignment, spacing: CGFloat?, content: Content
    public init(alignment: HorizontalAlignment = .center, spacing: CGFloat? = nil, @ViewBuilder content: () -> Content) {
        self.alignment = alignment; self.spacing = spacing; self.content = content()
    }
    func _emit(_ path: String) -> Node {
        ["kind": "vstack", "id": path, "spacing": spacing.map { _num($0) } ?? NSNull(), "alignment": alignment.name, "children": _children(content, path)]
    }
}

public struct HStack<Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let alignment: VerticalAlignment, spacing: CGFloat?, content: Content
    public init(alignment: VerticalAlignment = .center, spacing: CGFloat? = nil, @ViewBuilder content: () -> Content) {
        self.alignment = alignment; self.spacing = spacing; self.content = content()
    }
    func _emit(_ path: String) -> Node {
        ["kind": "hstack", "id": path, "spacing": spacing.map { _num($0) } ?? NSNull(), "alignment": alignment.name, "children": _children(content, path)]
    }
}

public struct ZStack<Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let alignment: Alignment, content: Content
    public init(alignment: Alignment = .center, @ViewBuilder content: () -> Content) {
        self.alignment = alignment; self.content = content()
    }
    func _emit(_ path: String) -> Node {
        ["kind": "zstack", "id": path, "alignment": alignment.name, "children": _children(content, path)]
    }
}

// MARK: - Alignment / Edge

public struct HorizontalAlignment: Equatable {
    let name: String
    public static let leading = HorizontalAlignment(name: "leading")
    public static let center = HorizontalAlignment(name: "center")
    public static let trailing = HorizontalAlignment(name: "trailing")
}
public struct VerticalAlignment: Equatable {
    let name: String
    public static let top = VerticalAlignment(name: "top")
    public static let center = VerticalAlignment(name: "center")
    public static let bottom = VerticalAlignment(name: "bottom")
}
public struct Alignment: Equatable {
    public let horizontal: HorizontalAlignment
    public let vertical: VerticalAlignment
    public init(horizontal: HorizontalAlignment, vertical: VerticalAlignment) { self.horizontal = horizontal; self.vertical = vertical }
    public static let center = Alignment(horizontal: .center, vertical: .center)
    public static let leading = Alignment(horizontal: .leading, vertical: .center)
    public static let trailing = Alignment(horizontal: .trailing, vertical: .center)
    public static let top = Alignment(horizontal: .center, vertical: .top)
    public static let bottom = Alignment(horizontal: .center, vertical: .bottom)
    public static let topLeading = Alignment(horizontal: .leading, vertical: .top)
    public static let topTrailing = Alignment(horizontal: .trailing, vertical: .top)
    public static let bottomLeading = Alignment(horizontal: .leading, vertical: .bottom)
    public static let bottomTrailing = Alignment(horizontal: .trailing, vertical: .bottom)
    var name: String {
        switch (vertical.name, horizontal.name) {
        case ("center", "center"): return "center"
        case ("center", let h): return h
        case (let v, "center"): return v
        case (let v, let h): return v + h.prefix(1).uppercased() + h.dropFirst()
        }
    }
}

public enum Edge: Int8, CaseIterable {
    case top = 1, leading = 2, bottom = 4, trailing = 8
    public struct Set: OptionSet {
        public let rawValue: Int8
        public init(rawValue: Int8) { self.rawValue = rawValue }
        public static let top = Set(rawValue: 1)
        public static let leading = Set(rawValue: 2)
        public static let bottom = Set(rawValue: 4)
        public static let trailing = Set(rawValue: 8)
        public static let horizontal: Set = [.leading, .trailing]
        public static let vertical: Set = [.top, .bottom]
        public static let all: Set = [.top, .leading, .bottom, .trailing]
    }
}

// MARK: - Color

public struct Color: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let json: Node
    init(json: Node) { self.json = json }
    public init(red: Double, green: Double, blue: Double, opacity: Double = 1) {
        json = ["r": red, "g": green, "b": blue, "a": opacity]
    }
    public init(white: Double, opacity: Double = 1) { self.init(red: white, green: white, blue: white, opacity: opacity) }
    func _emit(_ path: String) -> Node { ["kind": "color", "id": path, "color": json] }
    /// System color with a distinct dark-appearance rgb (0...255).
    static func _system(_ l: (Double, Double, Double), dark d: (Double, Double, Double)) -> Color {
        Color(json: ["r": l.0 / 255, "g": l.1 / 255, "b": l.2 / 255, "a": 1.0, "dark": [d.0 / 255, d.1 / 255, d.2 / 255]])
    }

    // iOS 26 system palette (light), sampled from the iOS 26.5 Simulator.
    public static let red = _system((255, 56, 60), dark: (255, 66, 69))
    public static let green = _system((52, 199, 89), dark: (48, 209, 88))
    public static let blue = _system((0, 136, 255), dark: (0, 145, 255))
    public static let orange = _system((255, 141, 40), dark: (255, 146, 48))
    public static let yellow = _system((255, 204, 0), dark: (255, 214, 0))
    public static let purple = _system((203, 48, 224), dark: (219, 52, 242))
    public static let pink = _system((255, 45, 85), dark: (255, 55, 95))
    public static let gray = _system((142, 142, 147), dark: (142, 142, 147))
    public static let black = Color(red: 0, green: 0, blue: 0)
    public static let white = Color(red: 1, green: 1, blue: 1)
    public static let clear = Color(red: 0, green: 0, blue: 0, opacity: 0)
    public static let primary = Color(json: ["semantic": "primary"])
    public static let secondary = Color(json: ["semantic": "secondary"])
    public static let accentColor = Color(json: ["semantic": "accent"])
}

// MARK: - Modifiers

func _num(_ v: CGFloat) -> Any { v.isInfinite ? "inf" : Double(v) }

struct _ModifiedView<Content: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content
    let modifier: (String) -> Node   // lazy: `background`/`overlay` evaluate their view at emit time
    func _children(_ path: String) -> [Node] {
        _modTargets(content, path).map { var n = $0; n["modifiers"] = ((n["modifiers"] as? [Node]) ?? []) + [modifier(path)]; return n }
    }
}

/// A multi-child `Group` hands a modifier to each child (SwiftUI semantics); everything else is evaluated to one node.
protocol _ModifierTransparent {}
func _modTargets<C: View>(_ c: C, _ path: String) -> [Node] {
    if c is _ModifierTransparent { let k = _children(c, path); if !k.isEmpty { return k } }
    return [_eval(c, path)]
}

extension View {
    func _modified(_ m: @escaping () -> Node) -> some View { _ModifiedView(content: self, modifier: { _ in m() }) }
    func _modifiedAt(_ m: @escaping (String) -> Node) -> some View { _ModifiedView(content: self, modifier: m) }

    public func padding() -> some View { padding(.all, 16) }
    public func padding(_ length: CGFloat) -> some View { padding(.all, length) }
    public func padding(_ edges: Edge.Set, _ length: CGFloat? = nil) -> some View {
        let l = Double(length ?? 16)
        return _modified { ["padding": ["top": edges.contains(.top) ? l : 0, "leading": edges.contains(.leading) ? l : 0,
                                        "bottom": edges.contains(.bottom) ? l : 0, "trailing": edges.contains(.trailing) ? l : 0]] }
    }
    public func frame(width: CGFloat? = nil, height: CGFloat? = nil, alignment: Alignment = .center) -> some View {
        _modified { ["frame": _frame(width, height, nil, nil, nil, nil, alignment)] }
    }
    public func frame(minWidth: CGFloat? = nil, maxWidth: CGFloat? = nil, minHeight: CGFloat? = nil, maxHeight: CGFloat? = nil, alignment: Alignment = .center) -> some View {
        _modified { ["frame": _frame(nil, nil, minWidth, maxWidth, minHeight, maxHeight, alignment)] }
    }
    public func foregroundStyle<S: ShapeStyle>(_ style: S) -> some View { _modified { ["foregroundColor": style._json] } }
    public func foregroundColor(_ color: Color?) -> some View {
        _modified { ["foregroundColor": color?.json ?? ["semantic": "primary"]] }
    }
    public func background<B: View>(_ view: B, alignment: Alignment = .center) -> some View {
        _BackgroundView(content: self, background: view)
    }
    public func cornerRadius(_ radius: CGFloat) -> some View { _modified { ["cornerRadius": Double(radius)] } }
    public func opacity(_ opacity: Double) -> some View { _modified { ["opacity": opacity] } }
}

func _frame(_ w: CGFloat?, _ h: CGFloat?, _ minW: CGFloat?, _ maxW: CGFloat?, _ minH: CGFloat?, _ maxH: CGFloat?, _ a: Alignment) -> Node {
    var f: Node = ["alignment": a.name]
    for (k, v) in [("width", w), ("height", h), ("minWidth", minW), ("maxWidth", maxW), ("minHeight", minH), ("maxHeight", maxH)] {
        if let v { f[k] = _num(v) }
    }
    return f
}

struct _BackgroundView<Content: View, B: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content
    let background: B
    func _children(_ path: String) -> [Node] {
        _modTargets(content, path).enumerated().map { i, c in
            var n = c
            n["modifiers"] = ((n["modifiers"] as? [Node]) ?? []) + [["background": _eval(background, path + (i == 0 ? "/bg" : "/bg\(i)"))]]
            return n
        }
    }
}

public struct PinnedScrollableViews: OptionSet {
    public let rawValue: Int
    public init(rawValue: Int) { self.rawValue = rawValue }
    public static let sectionHeaders = PinnedScrollableViews(rawValue: 1)
    public static let sectionFooters = PinnedScrollableViews(rawValue: 2)
}
