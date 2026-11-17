import Foundation

public struct Text: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let content: String
    public init(_ content: String) { self.content = content }
    public init(verbatim content: String) { self.content = content }
    func _emit(_ path: String) -> Node { ["kind": "text", "id": path, "text": content] }
}

public struct Spacer: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let minLength: CGFloat?
    public init(minLength: CGFloat? = nil) { self.minLength = minLength }
    func _emit(_ path: String) -> Node { ["kind": "spacer", "id": path, "minLength": minLength.map { _num($0) } ?? NSNull()] }
}


public struct Button<Label: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let action: () -> Void
    let label: Label
    public init(action: @escaping () -> Void, @ViewBuilder label: () -> Label) { self.action = action; self.label = label() }
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared
        return ["kind": "button", "id": path, "action": rt.disabled ? 0 : rt.registerAction(action), "label": _eval(label, path + "/0"),
                "style": rt.buttonStyle, "disabled": rt.disabled]
    }
}
extension Button where Label == Text {
    public init(_ title: String, action: @escaping () -> Void) { self.init(action: action) { Text(title) } }
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
        var n = _eval(content, path)
        n["modifiers"] = ((n["modifiers"] as? [Node]) ?? []) + [modifier(path)]
        return [n]
    }
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
        var n = _eval(content, path)
        n["modifiers"] = ((n["modifiers"] as? [Node]) ?? []) + [["background": _eval(background, path + "/bg")]]
        return [n]
    }
}
