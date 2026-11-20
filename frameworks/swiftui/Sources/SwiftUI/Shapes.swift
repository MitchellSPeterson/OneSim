#if !os(WASI)
import Foundation
#endif

// MARK: - ShapeStyle

public protocol ShapeStyle { var _json: [String: Any] { get } }

extension Color: ShapeStyle { public var _json: [String: Any] { json } }

public struct HierarchicalShapeStyle: ShapeStyle {
    let name: String
    public var _json: [String: Any] { ["semantic": name] }
}
extension ShapeStyle where Self == HierarchicalShapeStyle {
    public static var primary: HierarchicalShapeStyle { .init(name: "primary") }
    public static var secondary: HierarchicalShapeStyle { .init(name: "secondary") }
    public static var tertiary: HierarchicalShapeStyle { .init(name: "tertiary") }
}
extension ShapeStyle where Self == Color {
    public static var red: Color { Color.red }
    public static var green: Color { Color.green }
    public static var blue: Color { Color.blue }
    public static var orange: Color { Color.orange }
    public static var yellow: Color { Color.yellow }
    public static var purple: Color { Color.purple }
    public static var pink: Color { Color.pink }
    public static var gray: Color { Color.gray }
    public static var black: Color { Color.black }
    public static var white: Color { Color.white }
    public static var clear: Color { Color.clear }
}

extension Color {
    /// Scales alpha of RGBA colors. ponytail: semantic colors ignore opacity (no wire field for it).
    public func opacity(_ opacity: Double) -> Color {
        var j = json
        if let a = j["a"] as? Double { j["a"] = a * opacity }
        return Color(json: j)
    }
}

// MARK: - Shapes

public protocol Shape: View where Body == Never { var _shapeName: String { get } }
extension Shape {
    public var body: Never { fatalError() }
    public func fill<S: ShapeStyle>(_ style: S) -> some View { _ShapeView(shape: self, fill: style._json, stroke: nil) }
    public func stroke<S: ShapeStyle>(_ style: S, lineWidth: CGFloat = 1) -> some View {
        _ShapeView(shape: self, fill: nil, stroke: (style._json, Double(lineWidth)))
    }
}

func _shapeNode<S: Shape>(_ s: S, _ path: String, fill: Node?, stroke: (Node, Double)?) -> Node {
    // ponytail: the wire has no ellipse shape view yet; draw it as a capsule (glass uses the real name).
    let name = s._shapeName == "ellipse" ? "capsule" : s._shapeName
    var n: Node = ["kind": name == "rectangle" && fill == nil && stroke == nil ? "rectangle" : "shape", "id": path, "shape": name]
    if let r = (s as? RoundedRectangle)?.cornerRadius { n["cornerRadius"] = Double(r) }
    if let fill { n["fill"] = fill }
    if let stroke { n["stroke"] = ["color": stroke.0, "lineWidth": stroke.1] as Node }
    return n
}

struct _ShapeView<S: Shape>: View, _Primitive {
    typealias Body = Never
    var body: Never { fatalError() }
    let shape: S, fill: Node?, stroke: (Node, Double)?
    func _emit(_ path: String) -> Node { _shapeNode(shape, path, fill: fill, stroke: stroke) }
}

public struct Rectangle: Shape, _Primitive {
    public var _shapeName: String { "rectangle" }
    public init() {}
    func _emit(_ path: String) -> Node { _shapeNode(self, path, fill: nil, stroke: nil) }
}
public struct Circle: Shape, _Primitive {
    public var _shapeName: String { "circle" }
    public init() {}
    func _emit(_ path: String) -> Node { _shapeNode(self, path, fill: nil, stroke: nil) }
}
public struct Capsule: Shape, _Primitive {
    public var _shapeName: String { "capsule" }
    public init() {}
    func _emit(_ path: String) -> Node { _shapeNode(self, path, fill: nil, stroke: nil) }
}
public struct RoundedRectangle: Shape, _Primitive {
    public var _shapeName: String { "roundedRectangle" }
    public let cornerRadius: CGFloat
    public init(cornerRadius: CGFloat) { self.cornerRadius = cornerRadius }
    func _emit(_ path: String) -> Node { _shapeNode(self, path, fill: nil, stroke: nil) }
}

// MARK: - Font and text modifiers

public struct Font {
    let json: Node
    public struct Weight {
        let name: String
        public static let ultraLight = Weight(name: "ultraLight"), thin = Weight(name: "thin"), light = Weight(name: "light")
        public static let regular = Weight(name: "regular"), medium = Weight(name: "medium"), semibold = Weight(name: "semibold")
        public static let bold = Weight(name: "bold"), heavy = Weight(name: "heavy"), black = Weight(name: "black")
    }
    public enum Design { case `default`, serif, rounded, monospaced }
    /// Numeric width. Statics: compressed -0.3, condensed -0.2, standard 0, expanded 0.3.
    public struct Width: Hashable {
        public var value: CGFloat
        public static let compressed = Width(-0.3)
        public static let condensed = Width(-0.2)
        public static let standard = Width(0)
        public static let expanded = Width(0.3)
        public init(_ value: CGFloat) { self.value = value }
        /// <= -0.25 compressed, < -0.05 condensed, > 0.05 expanded, else standard.
        var _wire: String {
            if value <= -0.25 { return "compressed" }
            if value < -0.05 { return "condensed" }
            if value > 0.05 { return "expanded" }
            return "standard"
        }
    }
    static func style(_ s: String) -> Font { Font(json: ["style": s]) }
    public static let largeTitle = style("largeTitle"), title = style("title"), title2 = style("title2"), title3 = style("title3")
    public static let headline = style("headline"), subheadline = style("subheadline"), body = style("body"), callout = style("callout")
    public static let footnote = style("footnote"), caption = style("caption"), caption2 = style("caption2")
    public static func system(size: CGFloat, weight: Weight = .regular, design: Design = .default) -> Font {
        var j: Node = ["size": Double(size), "weight": weight.name]
        // Omit `.default` so a system font without a design still decodes as the old JSON.
        if case .default = design {} else { j["design"] = design._wire }
        return Font(json: j)
    }
    public func weight(_ weight: Weight) -> Font { var j = json; j["weight"] = weight.name; return Font(json: j) }
    public func bold() -> Font { weight(.bold) }
}

extension Font.Design {
    var _wire: String {
        switch self {
        case .default: return "default"
        case .serif: return "serif"
        case .rounded: return "rounded"
        case .monospaced: return "monospaced"
        }
    }
}

public enum TextAlignment { case leading, center, trailing }

public struct NavigationBarItem {
    public enum TitleDisplayMode { case automatic, inline, large }
}

// MARK: - Button styles

public protocol PrimitiveButtonStyle { var _styleName: String { get } }
public struct DefaultButtonStyle: PrimitiveButtonStyle { public var _styleName: String { "automatic" } }
public struct PlainButtonStyle: PrimitiveButtonStyle { public var _styleName: String { "plain" } }
public struct BorderedButtonStyle: PrimitiveButtonStyle { public var _styleName: String { "bordered" } }
public struct BorderedProminentButtonStyle: PrimitiveButtonStyle { public var _styleName: String { "borderedProminent" } }
extension PrimitiveButtonStyle where Self == DefaultButtonStyle { public static var automatic: DefaultButtonStyle { .init() } }
extension PrimitiveButtonStyle where Self == PlainButtonStyle { public static var plain: PlainButtonStyle { .init() } }
extension PrimitiveButtonStyle where Self == BorderedButtonStyle { public static var bordered: BorderedButtonStyle { .init() } }
extension PrimitiveButtonStyle where Self == BorderedProminentButtonStyle { public static var borderedProminent: BorderedProminentButtonStyle { .init() } }

/// Modifier that changes the evaluation context (disabled / button style) for its subtree and emits no node modifier.
struct _CtxView<Content: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content
    let apply: (_Runtime) -> Void
    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared
        #if os(WASI)
        let saved = (rt.disabled, rt.buttonStyle, rt.pickerStyle)
        apply(rt)
        defer { (rt.disabled, rt.buttonStyle, rt.pickerStyle) = saved }
        #else
        let saved = (rt.disabled, rt.buttonStyle, rt.pickerStyle, rt.imageSession)
        apply(rt)
        defer { (rt.disabled, rt.buttonStyle, rt.pickerStyle, rt.imageSession) = saved }
        #endif
        return SwiftUI._children(content, path)
    }
}

extension View {
    public func font(_ font: Font?) -> some View { _modified { ["font": font?.json ?? ["style": "body"] as Node] } }
    public func fontWeight(_ weight: Font.Weight?) -> some View { _modified { ["fontWeight": weight?.name ?? "regular"] } }
    public func bold() -> some View { _modified { ["fontWeight": "bold"] } }
    public func strikethrough(_ active: Bool = true) -> some View { _modified { ["strikethrough": active] } }
    /// No default on `pattern`: the bool overload already exists.
    public func strikethrough(_ isActive: Bool, pattern: Text.LineStyle.Pattern, color: Color? = nil) -> some View {
        _modified { ["strikethroughStyle": _lineStyle(isActive, pattern, color)] }
    }
    public func underline(_ isActive: Bool = true, pattern: Text.LineStyle.Pattern = .solid, color: Color? = nil) -> some View {
        _modified { ["underline": _lineStyle(isActive, pattern, color)] }
    }
    public func italic(_ isActive: Bool = true) -> some View { _modified { ["italic": isActive] } }
    public func kerning(_ kerning: CGFloat) -> some View { _modified { ["kerning": Double(kerning)] } }
    public func tracking(_ tracking: CGFloat) -> some View { _modified { ["tracking": Double(tracking)] } }
    public func baselineOffset(_ baselineOffset: CGFloat) -> some View { _modified { ["baselineOffset": Double(baselineOffset)] } }
    public func fontDesign(_ design: Font.Design?) -> some View { _modified { ["fontDesign": _orNull(design?._wire)] } }
    public func fontWidth(_ width: Font.Width?) -> some View { _modified { ["fontWidth": _orNull(width?._wire)] } }
    public func monospaced(_ isActive: Bool = true) -> some View { _modified { ["monospaced": isActive] } }
    public func monospacedDigit() -> some View { _modified { ["monospacedDigit": true] } }
    public func textScale(_ scale: Text.Scale, isEnabled: Bool = true) -> some View {
        _modified { ["textScale": scale == .secondary && isEnabled ? "secondary" : "default"] }
    }
    public func truncationMode(_ mode: Text.TruncationMode) -> some View { _modified { ["truncationMode": mode._wire] } }
    public func lineLimit(_ number: Int?) -> some View {
        _modified { ["lineLimit": number.map { ["max": $0] as Node } ?? NSNull()] }
    }
    public func lineLimit(_ limit: Int, reservesSpace: Bool) -> some View {
        var box: Node = ["max": limit]
        if reservesSpace { box["reserves"] = true }
        return _modified { ["lineLimit": box] }
    }
    public func lineLimit(_ limit: PartialRangeFrom<Int>) -> some View {
        _modified { ["lineLimit": ["min": limit.lowerBound]] }
    }
    public func lineLimit(_ limit: PartialRangeThrough<Int>) -> some View {
        _modified { ["lineLimit": ["max": limit.upperBound]] }
    }
    public func lineLimit(_ limit: ClosedRange<Int>) -> some View {
        _modified { ["lineLimit": ["min": limit.lowerBound, "max": limit.upperBound]] }
    }
    public func allowsTightening(_ flag: Bool) -> some View { _modified { ["allowsTightening": flag] } }
    public func minimumScaleFactor(_ factor: CGFloat) -> some View { _modified { ["minimumScaleFactor": Double(factor)] } }
    public func textCase(_ textCase: Text.Case?) -> some View { _modified { ["textCase": _orNull(textCase?._wire)] } }
    public func multilineTextAlignment(_ a: TextAlignment) -> some View {
        _modified { ["multilineTextAlignment": a == .leading ? "leading" : a == .center ? "center" : "trailing"] }
    }
    public func overlay<V: View>(alignment: Alignment = .center, @ViewBuilder content: () -> V) -> some View {
        overlay(content(), alignment: alignment)
    }
    public func overlay<V: View>(_ view: V, alignment: Alignment = .center) -> some View {
        _modifiedAt { ["overlay": ["view": _eval(view, $0 + "/ov"), "alignment": alignment.name] as Node] }
    }
    public func shadow(color: Color = Color(white: 0, opacity: 0.33), radius: CGFloat, x: CGFloat = 0, y: CGFloat = 0) -> some View {
        _modified { ["shadow": ["color": color.json, "radius": Double(radius), "x": Double(x), "y": Double(y)] as Node] }
    }
    public func navigationTitle(_ title: String) -> some View { _modified { ["navigationTitle": title] } }
    public func navigationBarTitleDisplayMode(_ mode: NavigationBarItem.TitleDisplayMode) -> some View {
        _modified { ["navigationBarTitleDisplayMode": mode == .inline ? "inline" : "large"] }   // lifted + stripped by NavigationStack
    }
    public func buttonStyle<S: PrimitiveButtonStyle>(_ style: S) -> some View {
        _CtxView(content: self) { $0.buttonStyle = style._styleName }
    }
    public func disabled(_ disabled: Bool) -> some View {
        _CtxView(content: self) { $0.disabled = $0.disabled || disabled }
    }
}

func _lineStyle(_ active: Bool, _ pattern: Text.LineStyle.Pattern, _ color: Color?) -> Node {
    var box: Node = ["active": active, "pattern": pattern._wire]
    if let color { box["color"] = color.json }
    return box
}
