#if os(WASI)
import WASILibc
#else
import Foundation
#endif
#if canImport(CoreGraphics)
import CoreGraphics
#endif

// MARK: - ShapeStyle

public protocol ShapeStyle { var _json: [String: Any] { get } }

extension Color: ShapeStyle { public var _json: [String: Any] { json } }

public struct ForegroundStyle: ShapeStyle, Equatable, Sendable {
    public init() {}
    public var _json: [String: Any] { [:] } // empty => inherit current foreground color on host
}

extension ShapeStyle where Self == ForegroundStyle {
    public static var foreground: ForegroundStyle { ForegroundStyle() }
}

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

// MARK: - Supporting Shape & Layout Types

public struct FillStyle: Equatable, Sendable {
    public var isEOFilled: Bool
    public var isAntialiased: Bool
    public init(eoFill: Bool = false, antialiased: Bool = true) {
        self.isEOFilled = eoFill
        self.isAntialiased = antialiased
    }
    var _json: Node? {
        guard isEOFilled || !isAntialiased else { return nil }
        var n: Node = [:]
        if isEOFilled { n["eoFill"] = true }
        if !isAntialiased { n["antialiased"] = false }
        return n
    }
}

public struct StrokeStyle: Equatable, Sendable {
    public var lineWidth: CGFloat
    public var lineCap: CGLineCap
    public var lineJoin: CGLineJoin
    public var miterLimit: CGFloat
    public var dash: [CGFloat]
    public var dashPhase: CGFloat

    public init(
        lineWidth: CGFloat = 1,
        lineCap: CGLineCap = .butt,
        lineJoin: CGLineJoin = .miter,
        miterLimit: CGFloat = 10,
        dash: [CGFloat] = [],
        dashPhase: CGFloat = 0
    ) {
        self.lineWidth = lineWidth
        self.lineCap = lineCap
        self.lineJoin = lineJoin
        self.miterLimit = miterLimit
        self.dash = dash
        self.dashPhase = dashPhase
    }

    func _json(color: Node?, antialiased: Bool, inset: CGFloat = 0) -> Node {
        var n: Node = ["lineWidth": Double(lineWidth)]
        if let color, !color.isEmpty { n["color"] = color }
        switch lineCap {
        case .round: n["lineCap"] = "round"
        case .square: n["lineCap"] = "square"
        default: break
        }
        switch lineJoin {
        case .round: n["lineJoin"] = "round"
        case .bevel: n["lineJoin"] = "bevel"
        default: break
        }
        if abs(miterLimit - 10) > 1e-6 { n["miterLimit"] = Double(miterLimit) }
        if !dash.isEmpty { n["dash"] = dash.map { Double($0) } }
        if dashPhase != 0 { n["dashPhase"] = Double(dashPhase) }
        if !antialiased { n["antialiased"] = false }
        if inset != 0 { n["inset"] = Double(inset) }
        return n
    }
}

public enum RoundedCornerStyle: Hashable, Sendable {
    case circular
    case continuous
    var _wire: String { self == .circular ? "circular" : "continuous" }
}

public struct RectangleCornerRadii: Equatable, Hashable, Sendable {
    public var topLeading: CGFloat
    public var bottomLeading: CGFloat
    public var bottomTrailing: CGFloat
    public var topTrailing: CGFloat

    public init(
        topLeading: CGFloat = 0,
        bottomLeading: CGFloat = 0,
        bottomTrailing: CGFloat = 0,
        topTrailing: CGFloat = 0
    ) {
        self.topLeading = topLeading
        self.bottomLeading = bottomLeading
        self.bottomTrailing = bottomTrailing
        self.topTrailing = topTrailing
    }
}

public struct ProposedViewSize: Equatable, Sendable {
    public var width: CGFloat?
    public var height: CGFloat?
    public static let zero = ProposedViewSize(width: 0, height: 0)
    public static let unspecified = ProposedViewSize(width: nil, height: nil)
    public static let infinity = ProposedViewSize(width: .infinity, height: .infinity)

    public init(width: CGFloat?, height: CGFloat?) {
        self.width = width
        self.height = height
    }

    public init(_ size: CGSize) {
        self.width = size.width
        self.height = size.height
    }

    public func replacingUnspecifiedDimensions(by size: CGSize = CGSize(width: 10, height: 10)) -> CGSize {
        CGSize(width: width ?? size.width, height: height ?? size.height)
    }
}

public enum ShapeRole: Hashable, Sendable {
    case fill
    case stroke
    case separator
}

public enum LayoutDirection: Hashable, CaseIterable, Sendable {
    case leftToRight
    case rightToLeft
}

public enum LayoutDirectionBehavior: Hashable, Sendable {
    case fixed
    case mirrors(in: LayoutDirection)
    public static var mirrors: LayoutDirectionBehavior { .mirrors(in: .rightToLeft) }
}

// MARK: - Edge.Corner & Edge.Corner.Style (iOS 26 ConcentricRectangle)

extension Edge {
    public enum Corner: Int8, Hashable, CaseIterable, Sendable {
        case topLeading = 1
        case topTrailing = 2
        case bottomLeading = 4
        case bottomTrailing = 8

        public struct Set: OptionSet, Hashable, Sendable {
            public let rawValue: Int8
            public init(rawValue: Int8) { self.rawValue = rawValue }
            public static let none = Set(rawValue: 0)
            public static let topLeading = Set(rawValue: 1)
            public static let topTrailing = Set(rawValue: 2)
            public static let bottomLeading = Set(rawValue: 4)
            public static let bottomTrailing = Set(rawValue: 8)
            public static let top: Set = [.topLeading, .topTrailing]
            public static let bottom: Set = [.bottomLeading, .bottomTrailing]
            public static let leading: Set = [.topLeading, .bottomLeading]
            public static let trailing: Set = [.topTrailing, .bottomTrailing]
            public static let all: Set = [.topLeading, .topTrailing, .bottomLeading, .bottomTrailing]
            public init(_ corner: Edge.Corner) { self.init(rawValue: corner.rawValue) }
            public func contains(_ corner: Edge.Corner) -> Bool { contains(Set(corner)) }
        }

        public struct Style: Hashable, Equatable, Sendable, ExpressibleByFloatLiteral, ExpressibleByIntegerLiteral {
            enum Storage: Hashable, Sendable {
                case fixed(CGFloat)
                case concentric(minimum: CGFloat?)
            }
            let storage: Storage

            init(_ storage: Storage) { self.storage = storage }
            public init(floatLiteral value: Float) { self.storage = .fixed(CGFloat(value)) }
            public init(integerLiteral value: Int) { self.storage = .fixed(CGFloat(value)) }

            public static func fixed(_ radius: CGFloat) -> Style { Style(.fixed(radius)) }
            public static var concentric: Style { Style(.concentric(minimum: nil)) }
            public static func concentric(minimum: Style? = nil) -> Style {
                let minVal: CGFloat?
                if case .fixed(let r) = minimum?.storage { minVal = r } else { minVal = nil }
                return Style(.concentric(minimum: minVal))
            }

            var _wire: Node {
                switch storage {
                case .fixed(let r): return ["fixed": Double(r)]
                case .concentric(let m): return ["concentric": m.map { Double($0) } ?? NSNull()]
                }
            }

            var _fallbackRadius: CGFloat {
                switch storage {
                case .fixed(let r): return max(0, r)
                case .concentric(let m): return max(0, m ?? 0)
                }
            }
        }
    }
}

public struct RoundedRectangularShapeCorners: Hashable, Sendable {
    public var topLeading: Edge.Corner.Style
    public var bottomLeading: Edge.Corner.Style
    public var topTrailing: Edge.Corner.Style
    public var bottomTrailing: Edge.Corner.Style

    public init(
        topLeading: Edge.Corner.Style = 0,
        topTrailing: Edge.Corner.Style = 0,
        bottomLeading: Edge.Corner.Style = 0,
        bottomTrailing: Edge.Corner.Style = 0
    ) {
        self.topLeading = topLeading
        self.topTrailing = topTrailing
        self.bottomLeading = bottomLeading
        self.bottomTrailing = bottomTrailing
    }

    public init(all: Edge.Corner.Style) {
        self.init(topLeading: all, topTrailing: all, bottomLeading: all, bottomTrailing: all)
    }

    public static func fixed(_ radius: CGFloat) -> RoundedRectangularShapeCorners {
        RoundedRectangularShapeCorners(all: .fixed(radius))
    }
    public static var concentric: RoundedRectangularShapeCorners {
        RoundedRectangularShapeCorners(all: .concentric)
    }
    public static func concentric(minimum: Edge.Corner.Style? = nil) -> RoundedRectangularShapeCorners {
        RoundedRectangularShapeCorners(all: .concentric(minimum: minimum))
    }

    public subscript(corner: Edge.Corner) -> Edge.Corner.Style {
        switch corner {
        case .topLeading: return topLeading
        case .topTrailing: return topTrailing
        case .bottomLeading: return bottomLeading
        case .bottomTrailing: return bottomTrailing
        }
    }
}

// MARK: - Shape Protocols

public protocol Shape: View {
    func path(in rect: CGRect) -> Path
    static var role: ShapeRole { get }
    var layoutDirectionBehavior: LayoutDirectionBehavior { get }
    func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize
}

public protocol InsettableShape: Shape {
    associatedtype InsetShape: InsettableShape
    func inset(by amount: CGFloat) -> Self.InsetShape
}

public protocol RoundedRectangularShape: InsettableShape {
    typealias Corners = RoundedRectangularShapeCorners
    func corners(in size: CGSize?) -> Self.Corners?
}

public protocol ShapeView<Content>: View {
    associatedtype Content: Shape
    var shape: Self.Content { get }
}

extension Shape {
    public static var role: ShapeRole { .fill }
    public var layoutDirectionBehavior: LayoutDirectionBehavior { .mirrors }
    public func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
        proposal.replacingUnspecifiedDimensions()
    }
    public var body: FillShapeView<Self, ForegroundStyle, EmptyView> {
        FillShapeView(shape: self, style: ForegroundStyle(), fillStyle: FillStyle(), background: EmptyView())
    }
}

// MARK: - Declarative vs Custom Shape Wire Emission

/// Built-in shapes that the host can draw in 1 pass without a `layout` round-trip.
protocol _DeclarativeShape {
    /// Returns base wire fields (`"shape"`, `"cornerRadius"`, etc.), or `nil` to fall back to `path(in:)`.
    func _declarativeFields() -> Node?
    /// Optional container-shape spec for `.containerShape(_:)`.
    func _containerSpec() -> Node?
}

extension _DeclarativeShape {
    func _containerSpec() -> Node? { _declarativeFields() }
}

private func _probeSizeHint<S: Shape>(_ shape: S) -> Any? {
    if shape is Circle { return "square" }
    let s1 = shape.sizeThatFits(ProposedViewSize(width: 100, height: 200))
    let s2 = shape.sizeThatFits(ProposedViewSize(width: 200, height: 100))
    // Default greedy Shape returns (100, 200) and (200, 100).
    if abs(s1.width - 100) < 1e-4 && abs(s1.height - 200) < 1e-4
        && abs(s2.width - 200) < 1e-4 && abs(s2.height - 100) < 1e-4 {
        let su = shape.sizeThatFits(.unspecified)
        if abs(su.width - 10) > 1e-4 || abs(su.height - 10) > 1e-4 {
            return ["ideal": [Double(su.width), Double(su.height)]]
        }
        return nil
    }
    if abs(s1.width - 100) < 1e-4 && abs(s1.height - 100) < 1e-4
        && abs(s2.width - 100) < 1e-4 && abs(s2.height - 100) < 1e-4 {
        let s3 = shape.sizeThatFits(ProposedViewSize(width: 120, height: 80))
        if abs(s3.width - 80) < 1e-4 && abs(s3.height - 80) < 1e-4 {
            return "square"
        }
        guard abs(s3.width - 100) < 1e-4 && abs(s3.height - 100) < 1e-4 else { return nil }
    }
    if abs(s1.width - s2.width) < 1e-4 && abs(s1.height - s2.height) < 1e-4 {
        return ["fixed": [Double(s1.width), Double(s1.height)]]
    }
    if s1.height > 1e-6 && s2.height > 1e-6 {
        let r1 = s1.width / s1.height, r2 = s2.width / s2.height
        if abs(r1 - r2) < 1e-4 {
            return ["aspect": Double(r1)]
        }
    }
    return nil
}

private func _shouldMirrorRTL<S: Shape>(_ shape: S) -> Bool {
    guard _Runtime.shared.layoutDirection == .rightToLeft else { return false }
    switch shape.layoutDirectionBehavior {
    case .fixed: return false
    case .mirrors(let dir): return dir == .rightToLeft
    }
}

func _emitShapeNode<S: Shape>(
    _ shape: S,
    path id: String,
    fill: Node?,
    fillStyle: FillStyle?,
    stroke: (color: Node, style: StrokeStyle, antialiased: Bool, inset: CGFloat)?
) -> Node {
    var n: Node
    let strokeInset: CGFloat
    if let decl = shape as? _DeclarativeShape, let fields = decl._declarativeFields() {
        n = fields
        let isPlainRect = (n["shape"] as? String) == "rectangle"
            && fill == nil && fillStyle?._json == nil && stroke == nil
        n["kind"] = isPlainRect ? "rectangle" : "shape"
        n["id"] = id
        strokeInset = stroke?.inset ?? 0
    } else {
        // Custom or composed shape: evaluate `path(in:)` at the host-reported laid-out size (protocol section N).
        n = ["kind": "shape", "id": id, "shape": "path", "custom": true]
        if let hint = _probeSizeHint(shape) { n["sizeHint"] = hint }
        if _shouldMirrorRTL(shape) { n["mirrorRtl"] = true }
        if let sz = _Runtime.shared.shapeSizes[id] {
            let built = shape.path(in: CGRect(origin: .zero, size: sz))
            n["path"] = built._wireOps()
            n["pathSize"] = [Double(sz.width), Double(sz.height)]
        }
        strokeInset = 0
    }
    if let fill, !fill.isEmpty { n["fill"] = fill }
    if let fs = fillStyle?._json { n["fillStyle"] = fs }
    if let st = stroke {
        n["stroke"] = st.style._json(color: st.color.isEmpty ? nil : st.color, antialiased: st.antialiased, inset: strokeInset)
    }
    return n
}

private func _attachBackground<B: View>(_ node: Node, _ background: B, _ path: String) -> Node {
    if background is EmptyView { return node }
    var n = node
    let bg = _eval(background, path + "/bg")
    n["modifiers"] = ((n["modifiers"] as? [Node]) ?? []) + [["background": bg]]
    return n
}

// MARK: - ShapeView Types

public struct FillShapeView<Content: Shape, Style: ShapeStyle, Background: View>: ShapeView, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    public var shape: Content
    public var style: Style
    public var fillStyle: FillStyle
    public var background: Background

    public init(shape: Content, style: Style, fillStyle: FillStyle, background: Background) {
        self.shape = shape
        self.style = style
        self.fillStyle = fillStyle
        self.background = background
    }

    func _emit(_ path: String) -> Node {
        let fillJson = style._json
        let node = _emitShapeNode(shape, path: path, fill: fillJson.isEmpty ? nil : fillJson, fillStyle: fillStyle, stroke: nil)
        return _attachBackground(node, background, path)
    }
}

public struct StrokeShapeView<Content: Shape, Style: ShapeStyle, Background: View>: ShapeView, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    public var shape: Content
    public var style: Style
    public var strokeStyle: StrokeStyle
    public var isAntialiased: Bool
    public var background: Background

    public init(shape: Content, style: Style, strokeStyle: StrokeStyle, isAntialiased: Bool, background: Background) {
        self.shape = shape
        self.style = style
        self.strokeStyle = strokeStyle
        self.isAntialiased = isAntialiased
        self.background = background
    }

    func _emit(_ path: String) -> Node {
        let node = _emitShapeNode(shape, path: path, fill: nil, fillStyle: nil, stroke: (style._json, strokeStyle, isAntialiased, 0))
        return _attachBackground(node, background, path)
    }
}

public struct StrokeBorderShapeView<Content: InsettableShape, Style: ShapeStyle, Background: View>: ShapeView, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    public var shape: Content
    public var style: Style
    public var strokeStyle: StrokeStyle
    public var isAntialiased: Bool
    public var background: Background

    public init(shape: Content, style: Style, strokeStyle: StrokeStyle, isAntialiased: Bool, background: Background) {
        self.shape = shape
        self.style = style
        self.strokeStyle = strokeStyle
        self.isAntialiased = isAntialiased
        self.background = background
    }

    func _emit(_ path: String) -> Node {
        let insetAmount = strokeStyle.lineWidth * 0.5
        if (shape as? _DeclarativeShape)?._declarativeFields() != nil {
            let node = _emitShapeNode(shape, path: path, fill: nil, fillStyle: nil, stroke: (style._json, strokeStyle, isAntialiased, insetAmount))
            return _attachBackground(node, background, path)
        }
        let insetShape = shape.inset(by: insetAmount)
        let node = _emitShapeNode(insetShape, path: path, fill: nil, fillStyle: nil, stroke: (style._json, strokeStyle, isAntialiased, 0))
        return _attachBackground(node, background, path)
    }
}

// MARK: - Shape / InsettableShape / ShapeView Modifiers

extension Shape {
    public func fill<S: ShapeStyle>(_ content: S, style: FillStyle = FillStyle()) -> FillShapeView<Self, S, EmptyView> {
        FillShapeView(shape: self, style: content, fillStyle: style, background: EmptyView())
    }
    public func fill(style: FillStyle = FillStyle()) -> FillShapeView<Self, ForegroundStyle, EmptyView> {
        FillShapeView(shape: self, style: ForegroundStyle(), fillStyle: style, background: EmptyView())
    }
    public func stroke<S: ShapeStyle>(_ content: S, style: StrokeStyle, antialiased: Bool = true) -> StrokeShapeView<Self, S, EmptyView> {
        StrokeShapeView(shape: self, style: content, strokeStyle: style, isAntialiased: antialiased, background: EmptyView())
    }
    public func stroke<S: ShapeStyle>(_ content: S, lineWidth: CGFloat = 1, antialiased: Bool = true) -> StrokeShapeView<Self, S, EmptyView> {
        StrokeShapeView(shape: self, style: content, strokeStyle: StrokeStyle(lineWidth: lineWidth), isAntialiased: antialiased, background: EmptyView())
    }
    public func stroke(style: StrokeStyle) -> some Shape {
        _StrokedShape(shape: self, style: style)
    }
    public func stroke(lineWidth: CGFloat = 1) -> some Shape {
        _StrokedShape(shape: self, style: StrokeStyle(lineWidth: lineWidth))
    }
}

extension InsettableShape {
    public func strokeBorder<S: ShapeStyle>(_ content: S, style: StrokeStyle, antialiased: Bool = true) -> StrokeBorderShapeView<Self, S, EmptyView> {
        StrokeBorderShapeView(shape: self, style: content, strokeStyle: style, isAntialiased: antialiased, background: EmptyView())
    }
    public func strokeBorder(style: StrokeStyle, antialiased: Bool = true) -> StrokeBorderShapeView<Self, ForegroundStyle, EmptyView> {
        StrokeBorderShapeView(shape: self, style: ForegroundStyle(), strokeStyle: style, isAntialiased: antialiased, background: EmptyView())
    }
    public func strokeBorder<S: ShapeStyle>(_ content: S, lineWidth: CGFloat = 1, antialiased: Bool = true) -> StrokeBorderShapeView<Self, S, EmptyView> {
        StrokeBorderShapeView(shape: self, style: content, strokeStyle: StrokeStyle(lineWidth: lineWidth), isAntialiased: antialiased, background: EmptyView())
    }
    public func strokeBorder(lineWidth: CGFloat = 1, antialiased: Bool = true) -> StrokeBorderShapeView<Self, ForegroundStyle, EmptyView> {
        StrokeBorderShapeView(shape: self, style: ForegroundStyle(), strokeStyle: StrokeStyle(lineWidth: lineWidth), isAntialiased: antialiased, background: EmptyView())
    }
}

extension ShapeView {
    public func fill<S: ShapeStyle>(_ content: S, style: FillStyle = FillStyle()) -> FillShapeView<Self.Content, S, Self> {
        FillShapeView(shape: shape, style: content, fillStyle: style, background: self)
    }
    public func fill(style: FillStyle = FillStyle()) -> FillShapeView<Self.Content, ForegroundStyle, Self> {
        FillShapeView(shape: shape, style: ForegroundStyle(), fillStyle: style, background: self)
    }
    public func stroke<S: ShapeStyle>(_ content: S, style: StrokeStyle, antialiased: Bool = true) -> StrokeShapeView<Self.Content, S, Self> {
        StrokeShapeView(shape: shape, style: content, strokeStyle: style, isAntialiased: antialiased, background: self)
    }
    public func stroke(style: StrokeStyle, antialiased: Bool = true) -> StrokeShapeView<Self.Content, ForegroundStyle, Self> {
        StrokeShapeView(shape: shape, style: ForegroundStyle(), strokeStyle: style, isAntialiased: antialiased, background: self)
    }
    public func stroke<S: ShapeStyle>(_ content: S, lineWidth: CGFloat = 1, antialiased: Bool = true) -> StrokeShapeView<Self.Content, S, Self> {
        StrokeShapeView(shape: shape, style: content, strokeStyle: StrokeStyle(lineWidth: lineWidth), isAntialiased: antialiased, background: self)
    }
    public func stroke(lineWidth: CGFloat = 1, antialiased: Bool = true) -> StrokeShapeView<Self.Content, ForegroundStyle, Self> {
        StrokeShapeView(shape: shape, style: ForegroundStyle(), strokeStyle: StrokeStyle(lineWidth: lineWidth), isAntialiased: antialiased, background: self)
    }
}

extension ShapeView where Content: InsettableShape {
    public func strokeBorder<S: ShapeStyle>(_ content: S, style: StrokeStyle, antialiased: Bool = true) -> StrokeBorderShapeView<Self.Content, S, Self> {
        StrokeBorderShapeView(shape: shape, style: content, strokeStyle: style, isAntialiased: antialiased, background: self)
    }
    public func strokeBorder(style: StrokeStyle, antialiased: Bool = true) -> StrokeBorderShapeView<Self.Content, ForegroundStyle, Self> {
        StrokeBorderShapeView(shape: shape, style: ForegroundStyle(), strokeStyle: style, isAntialiased: antialiased, background: self)
    }
    public func strokeBorder<S: ShapeStyle>(_ content: S, lineWidth: CGFloat = 1, antialiased: Bool = true) -> StrokeBorderShapeView<Self.Content, S, Self> {
        StrokeBorderShapeView(shape: shape, style: content, strokeStyle: StrokeStyle(lineWidth: lineWidth), isAntialiased: antialiased, background: self)
    }
    public func strokeBorder(lineWidth: CGFloat = 1, antialiased: Bool = true) -> StrokeBorderShapeView<Self.Content, ForegroundStyle, Self> {
        StrokeBorderShapeView(shape: shape, style: ForegroundStyle(), strokeStyle: StrokeStyle(lineWidth: lineWidth), isAntialiased: antialiased, background: self)
    }
}

// MARK: - Path as Shape

extension Path: Shape, _DeclarativeShape {
    public func path(in _: CGRect) -> Path { self }
    func _declarativeFields() -> Node? {
        ["shape": "path", "path": _wireOps()]
    }
}

// MARK: - Built-in Shapes

public struct Rectangle: Shape, InsettableShape, RoundedRectangularShape, _DeclarativeShape, Sendable {
    var insetAmount: CGFloat = 0
    public init() {}
    init(insetAmount: CGFloat) { self.insetAmount = insetAmount }
    public var layoutDirectionBehavior: LayoutDirectionBehavior { .fixed }
    public func path(in rect: CGRect) -> Path {
        let r = rect.insetBy(dx: insetAmount, dy: insetAmount)
        return r.isNull ? Path() : Path(r)
    }
    public func inset(by amount: CGFloat) -> Rectangle { Rectangle(insetAmount: insetAmount + amount) }
    public func corners(in size: CGSize?) -> RoundedRectangularShapeCorners? { RoundedRectangularShapeCorners(all: .fixed(0)) }
    func _declarativeFields() -> Node? {
        insetAmount == 0 ? ["shape": "rectangle"] : nil
    }
}

public struct Circle: Shape, InsettableShape, RoundedRectangularShape, _DeclarativeShape, Sendable {
    var insetAmount: CGFloat = 0
    public init() {}
    init(insetAmount: CGFloat) { self.insetAmount = insetAmount }
    public var layoutDirectionBehavior: LayoutDirectionBehavior { .fixed }
    public func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
        let side: CGFloat
        switch (proposal.width, proposal.height) {
        case (let w?, let h?): side = min(w, h)
        case (let w?, nil): side = w
        case (nil, let h?): side = h
        case (nil, nil): side = 10
        }
        return CGSize(width: side, height: side)
    }
    public func path(in rect: CGRect) -> Path {
        let r = rect.standardized
        let side = min(r.width, r.height)
        let sq = CGRect(x: r.midX - side / 2, y: r.midY - side / 2, width: side, height: side).insetBy(dx: insetAmount, dy: insetAmount)
        return sq.isNull ? Path() : Path(ellipseIn: sq)
    }
    public func inset(by amount: CGFloat) -> Circle { Circle(insetAmount: insetAmount + amount) }
    public func corners(in size: CGSize?) -> RoundedRectangularShapeCorners? {
        guard let s = size else { return nil }
        return .fixed(min(s.width, s.height) / 2)
    }
    func _declarativeFields() -> Node? {
        insetAmount == 0 ? ["shape": "circle"] : nil
    }
}

public struct Ellipse: Shape, InsettableShape, _DeclarativeShape, Sendable {
    var insetAmount: CGFloat = 0
    public init() {}
    init(insetAmount: CGFloat) { self.insetAmount = insetAmount }
    public var layoutDirectionBehavior: LayoutDirectionBehavior { .fixed }
    public func path(in rect: CGRect) -> Path {
        let r = rect.insetBy(dx: insetAmount, dy: insetAmount)
        return r.isNull ? Path() : Path(ellipseIn: r)
    }
    public func inset(by amount: CGFloat) -> Ellipse { Ellipse(insetAmount: insetAmount + amount) }
    func _declarativeFields() -> Node? {
        insetAmount == 0 ? ["shape": "ellipse"] : nil
    }
}

public struct Capsule: Shape, InsettableShape, RoundedRectangularShape, _DeclarativeShape, Sendable {
    public var style: RoundedCornerStyle
    var insetAmount: CGFloat = 0
    public init(style: RoundedCornerStyle = .continuous) { self.style = style }
    init(style: RoundedCornerStyle, insetAmount: CGFloat) { self.style = style; self.insetAmount = insetAmount }
    public var layoutDirectionBehavior: LayoutDirectionBehavior { .fixed }
    public func path(in rect: CGRect) -> Path {
        let r = rect.insetBy(dx: insetAmount, dy: insetAmount)
        guard !r.isNull else { return Path() }
        let rad = min(r.width, r.height) / 2
        return Path(roundedRect: r, cornerRadius: rad, style: style)
    }
    public func inset(by amount: CGFloat) -> Capsule { Capsule(style: style, insetAmount: insetAmount + amount) }
    public func corners(in size: CGSize?) -> RoundedRectangularShapeCorners? {
        guard let s = size else { return nil }
        return .fixed(min(s.width, s.height) / 2)
    }
    func _declarativeFields() -> Node? {
        guard insetAmount == 0 else { return nil }
        return ["shape": "capsule", "cornerStyle": style._wire]
    }
}

public struct RoundedRectangle: Shape, InsettableShape, RoundedRectangularShape, _DeclarativeShape, Sendable {
    public var cornerSize: CGSize
    public var style: RoundedCornerStyle
    var insetAmount: CGFloat = 0

    public init(cornerSize: CGSize, style: RoundedCornerStyle = .continuous) {
        self.cornerSize = cornerSize
        self.style = style
    }

    public init(cornerRadius: CGFloat, style: RoundedCornerStyle = .continuous) {
        self.cornerSize = CGSize(width: cornerRadius, height: cornerRadius)
        self.style = style
    }

    init(cornerSize: CGSize, style: RoundedCornerStyle, insetAmount: CGFloat) {
        self.cornerSize = cornerSize; self.style = style; self.insetAmount = insetAmount
    }

    public var layoutDirectionBehavior: LayoutDirectionBehavior { .fixed }
    public func path(in rect: CGRect) -> Path {
        let r = rect.insetBy(dx: insetAmount, dy: insetAmount)
        guard !r.isNull else { return Path() }
        let cs = CGSize(width: max(0, cornerSize.width - insetAmount), height: max(0, cornerSize.height - insetAmount))
        return Path(roundedRect: r, cornerSize: cs, style: style)
    }
    public func inset(by amount: CGFloat) -> RoundedRectangle {
        RoundedRectangle(cornerSize: cornerSize, style: style, insetAmount: insetAmount + amount)
    }
    public func corners(in size: CGSize?) -> RoundedRectangularShapeCorners? {
        .fixed(min(cornerSize.width, cornerSize.height))
    }
    func _declarativeFields() -> Node? {
        guard insetAmount == 0 else { return nil }
        var n: Node = ["shape": "roundedRectangle", "cornerStyle": style._wire]
        if abs(cornerSize.width - cornerSize.height) > 1e-6 {
            n["cornerSize"] = [Double(cornerSize.width), Double(cornerSize.height)]
        } else {
            n["cornerRadius"] = Double(cornerSize.width)
        }
        return n
    }
}

public struct UnevenRoundedRectangle: Shape, InsettableShape, RoundedRectangularShape, _DeclarativeShape, Sendable {
    public var cornerRadii: RectangleCornerRadii
    public var style: RoundedCornerStyle
    var insetAmount: CGFloat = 0

    public init(cornerRadii: RectangleCornerRadii, style: RoundedCornerStyle = .continuous) {
        self.cornerRadii = cornerRadii
        self.style = style
    }

    public init(
        topLeadingRadius: CGFloat = 0,
        bottomLeadingRadius: CGFloat = 0,
        bottomTrailingRadius: CGFloat = 0,
        topTrailingRadius: CGFloat = 0,
        style: RoundedCornerStyle = .continuous
    ) {
        self.cornerRadii = RectangleCornerRadii(
            topLeading: topLeadingRadius,
            bottomLeading: bottomLeadingRadius,
            bottomTrailing: bottomTrailingRadius,
            topTrailing: topTrailingRadius
        )
        self.style = style
    }

    init(cornerRadii: RectangleCornerRadii, style: RoundedCornerStyle, insetAmount: CGFloat) {
        self.cornerRadii = cornerRadii; self.style = style; self.insetAmount = insetAmount
    }

    public func path(in rect: CGRect) -> Path {
        let r = rect.insetBy(dx: insetAmount, dy: insetAmount)
        guard !r.isNull else { return Path() }
        let cr = RectangleCornerRadii(
            topLeading: max(0, cornerRadii.topLeading - insetAmount),
            bottomLeading: max(0, cornerRadii.bottomLeading - insetAmount),
            bottomTrailing: max(0, cornerRadii.bottomTrailing - insetAmount),
            topTrailing: max(0, cornerRadii.topTrailing - insetAmount)
        )
        return Path(roundedRect: r, cornerRadii: cr, style: style)
    }

    public func inset(by amount: CGFloat) -> UnevenRoundedRectangle {
        UnevenRoundedRectangle(cornerRadii: cornerRadii, style: style, insetAmount: insetAmount + amount)
    }

    public func corners(in size: CGSize?) -> RoundedRectangularShapeCorners? {
        RoundedRectangularShapeCorners(
            topLeading: .fixed(cornerRadii.topLeading),
            topTrailing: .fixed(cornerRadii.topTrailing),
            bottomLeading: .fixed(cornerRadii.bottomLeading),
            bottomTrailing: .fixed(cornerRadii.bottomTrailing)
        )
    }

    func _declarativeFields() -> Node? {
        guard insetAmount == 0 else { return nil }
        return [
            "shape": "unevenRoundedRectangle",
            "cornerRadii": [
                Double(cornerRadii.topLeading),
                Double(cornerRadii.topTrailing),
                Double(cornerRadii.bottomTrailing),
                Double(cornerRadii.bottomLeading),
            ],
            "cornerStyle": style._wire,
        ]
    }
}

public struct ContainerRelativeShape: Shape, InsettableShape, _DeclarativeShape, Sendable {
    var insetAmount: CGFloat = 0
    public init() {}
    init(insetAmount: CGFloat) { self.insetAmount = insetAmount }
    public func path(in rect: CGRect) -> Path {
        let r = rect.insetBy(dx: insetAmount, dy: insetAmount)
        return r.isNull ? Path() : Path(r)
    }
    public func inset(by amount: CGFloat) -> ContainerRelativeShape {
        ContainerRelativeShape(insetAmount: insetAmount + amount)
    }
    func _declarativeFields() -> Node? {
        insetAmount == 0 ? ["shape": "containerRelative"] : nil
    }
}

public struct ConcentricRectangle: Shape, InsettableShape, _DeclarativeShape, Sendable {
    enum _UniformGroup: String, Sendable {
        case all, top, bottom, leading, trailing, topAndBottom, leadingAndTrailing
    }
    let topLeading: Edge.Corner.Style
    let topTrailing: Edge.Corner.Style
    let bottomTrailing: Edge.Corner.Style
    let bottomLeading: Edge.Corner.Style
    let uniform: _UniformGroup?
    var insetAmount: CGFloat = 0

    public init() {
        self.init(topLeadingCorner: .concentric, topTrailingCorner: .concentric, bottomLeadingCorner: .concentric, bottomTrailingCorner: .concentric)
    }

    public init(corners: Edge.Corner.Style, isUniform: Bool = false) {
        self.topLeading = corners; self.topTrailing = corners
        self.bottomTrailing = corners; self.bottomLeading = corners
        self.uniform = isUniform ? .all : nil
    }

    public init(
        topLeadingCorner: Edge.Corner.Style = .concentric,
        topTrailingCorner: Edge.Corner.Style = .concentric,
        bottomLeadingCorner: Edge.Corner.Style = .concentric,
        bottomTrailingCorner: Edge.Corner.Style = .concentric
    ) {
        self.topLeading = topLeadingCorner; self.topTrailing = topTrailingCorner
        self.bottomTrailing = bottomTrailingCorner; self.bottomLeading = bottomLeadingCorner
        self.uniform = nil
    }

    public init(uniformTopCorners: Edge.Corner.Style = .concentric, uniformBottomCorners: Edge.Corner.Style = .concentric) {
        self.topLeading = uniformTopCorners; self.topTrailing = uniformTopCorners
        self.bottomTrailing = uniformBottomCorners; self.bottomLeading = uniformBottomCorners
        self.uniform = .topAndBottom
    }

    public init(uniformLeadingCorners: Edge.Corner.Style = .concentric, uniformTrailingCorners: Edge.Corner.Style = .concentric) {
        self.topLeading = uniformLeadingCorners; self.topTrailing = uniformTrailingCorners
        self.bottomTrailing = uniformTrailingCorners; self.bottomLeading = uniformLeadingCorners
        self.uniform = .leadingAndTrailing
    }

    public init(
        uniformTopCorners: Edge.Corner.Style = .concentric,
        bottomLeadingCorner: Edge.Corner.Style = .concentric,
        bottomTrailingCorner: Edge.Corner.Style = .concentric
    ) {
        self.topLeading = uniformTopCorners; self.topTrailing = uniformTopCorners
        self.bottomTrailing = bottomTrailingCorner; self.bottomLeading = bottomLeadingCorner
        self.uniform = .top
    }

    public init(
        uniformBottomCorners: Edge.Corner.Style = .concentric,
        topLeadingCorner: Edge.Corner.Style = .concentric,
        topTrailingCorner: Edge.Corner.Style = .concentric
    ) {
        self.topLeading = topLeadingCorner; self.topTrailing = topTrailingCorner
        self.bottomTrailing = uniformBottomCorners; self.bottomLeading = uniformBottomCorners
        self.uniform = .bottom
    }

    public init(
        uniformLeadingCorners: Edge.Corner.Style = .concentric,
        topTrailingCorner: Edge.Corner.Style = .concentric,
        bottomTrailingCorner: Edge.Corner.Style = .concentric
    ) {
        self.topLeading = uniformLeadingCorners; self.topTrailing = topTrailingCorner
        self.bottomTrailing = bottomTrailingCorner; self.bottomLeading = uniformLeadingCorners
        self.uniform = .leading
    }

    public init(
        uniformTrailingCorners: Edge.Corner.Style = .concentric,
        topLeadingCorner: Edge.Corner.Style = .concentric,
        bottomLeadingCorner: Edge.Corner.Style = .concentric
    ) {
        self.topLeading = topLeadingCorner; self.topTrailing = uniformTrailingCorners
        self.bottomTrailing = uniformTrailingCorners; self.bottomLeading = bottomLeadingCorner
        self.uniform = .trailing
    }

    init(
        topLeading: Edge.Corner.Style, topTrailing: Edge.Corner.Style,
        bottomTrailing: Edge.Corner.Style, bottomLeading: Edge.Corner.Style,
        uniform: _UniformGroup?, insetAmount: CGFloat
    ) {
        self.topLeading = topLeading; self.topTrailing = topTrailing
        self.bottomTrailing = bottomTrailing; self.bottomLeading = bottomLeading
        self.uniform = uniform; self.insetAmount = insetAmount
    }

    public func path(in rect: CGRect) -> Path {
        let r = rect.insetBy(dx: insetAmount, dy: insetAmount)
        guard !r.isNull else { return Path() }
        let cr = RectangleCornerRadii(
            topLeading: max(0, topLeading._fallbackRadius - insetAmount),
            bottomLeading: max(0, bottomLeading._fallbackRadius - insetAmount),
            bottomTrailing: max(0, bottomTrailing._fallbackRadius - insetAmount),
            topTrailing: max(0, topTrailing._fallbackRadius - insetAmount)
        )
        return Path(roundedRect: r, cornerRadii: cr, style: .continuous)
    }

    public func inset(by amount: CGFloat) -> ConcentricRectangle {
        ConcentricRectangle(
            topLeading: topLeading, topTrailing: topTrailing,
            bottomTrailing: bottomTrailing, bottomLeading: bottomLeading,
            uniform: uniform, insetAmount: insetAmount + amount
        )
    }

    func _declarativeFields() -> Node? {
        guard insetAmount == 0 else { return nil }
        var c: Node = ["corners": [topLeading._wire, topTrailing._wire, bottomTrailing._wire, bottomLeading._wire]]
        if let uniform { c["uniform"] = uniform.rawValue }
        return ["shape": "concentricRectangle", "concentric": c]
    }
}

// MARK: - AnyShape

public struct AnyShape: Shape, _DeclarativeShape {
    private let _path: (CGRect) -> Path
    private let _sizeThatFits: (ProposedViewSize) -> CGSize
    private let _behavior: LayoutDirectionBehavior
    private let _decl: () -> Node?

    public init<S: Shape>(_ shape: S) {
        self._path = { shape.path(in: $0) }
        self._sizeThatFits = { shape.sizeThatFits($0) }
        self._behavior = shape.layoutDirectionBehavior
        self._decl = { (shape as? _DeclarativeShape)?._declarativeFields() }
    }

    public var layoutDirectionBehavior: LayoutDirectionBehavior { _behavior }
    public func path(in rect: CGRect) -> Path { _path(rect) }
    public func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize { _sizeThatFits(proposal) }
    func _declarativeFields() -> Node? { _decl() }
}

// MARK: - Static Shape Shorthands

extension Shape where Self == Rectangle { public static var rect: Rectangle { Rectangle() } }
extension Shape where Self == Circle { public static var circle: Circle { Circle() } }
extension Shape where Self == Ellipse { public static var ellipse: Ellipse { Ellipse() } }
extension Shape where Self == Capsule {
    public static var capsule: Capsule { Capsule() }
    public static func capsule(style: RoundedCornerStyle) -> Capsule { Capsule(style: style) }
}
extension Shape where Self == RoundedRectangle {
    public static func rect(cornerRadius: CGFloat, style: RoundedCornerStyle = .continuous) -> RoundedRectangle {
        RoundedRectangle(cornerRadius: cornerRadius, style: style)
    }
    public static func rect(cornerSize: CGSize, style: RoundedCornerStyle = .continuous) -> RoundedRectangle {
        RoundedRectangle(cornerSize: cornerSize, style: style)
    }
}
extension Shape where Self == UnevenRoundedRectangle {
    public static func rect(cornerRadii: RectangleCornerRadii, style: RoundedCornerStyle = .continuous) -> UnevenRoundedRectangle {
        UnevenRoundedRectangle(cornerRadii: cornerRadii, style: style)
    }
    public static func rect(
        topLeadingRadius: CGFloat = 0,
        bottomLeadingRadius: CGFloat = 0,
        bottomTrailingRadius: CGFloat = 0,
        topTrailingRadius: CGFloat = 0,
        style: RoundedCornerStyle = .continuous
    ) -> UnevenRoundedRectangle {
        UnevenRoundedRectangle(
            topLeadingRadius: topLeadingRadius,
            bottomLeadingRadius: bottomLeadingRadius,
            bottomTrailingRadius: bottomTrailingRadius,
            topTrailingRadius: topTrailingRadius,
            style: style
        )
    }
}
extension Shape where Self == ContainerRelativeShape {
    public static var containerRelative: ContainerRelativeShape { ContainerRelativeShape() }
}
extension Shape where Self == ConcentricRectangle {
    public static func rect(corners: Edge.Corner.Style, isUniform: Bool = false) -> ConcentricRectangle {
        ConcentricRectangle(corners: corners, isUniform: isUniform)
    }
    public static func rect(
        topLeadingCorner: Edge.Corner.Style,
        topTrailingCorner: Edge.Corner.Style,
        bottomLeadingCorner: Edge.Corner.Style,
        bottomTrailingCorner: Edge.Corner.Style
    ) -> ConcentricRectangle {
        ConcentricRectangle(
            topLeadingCorner: topLeadingCorner,
            topTrailingCorner: topTrailingCorner,
            bottomLeadingCorner: bottomLeadingCorner,
            bottomTrailingCorner: bottomTrailingCorner
        )
    }
    public static func rect(uniformTopCorners: Edge.Corner.Style, uniformBottomCorners: Edge.Corner.Style) -> ConcentricRectangle {
        ConcentricRectangle(uniformTopCorners: uniformTopCorners, uniformBottomCorners: uniformBottomCorners)
    }
    public static func rect(uniformLeadingCorners: Edge.Corner.Style, uniformTrailingCorners: Edge.Corner.Style) -> ConcentricRectangle {
        ConcentricRectangle(uniformLeadingCorners: uniformLeadingCorners, uniformTrailingCorners: uniformTrailingCorners)
    }
    public static func rect(
        uniformTopCorners: Edge.Corner.Style,
        bottomLeadingCorner: Edge.Corner.Style,
        bottomTrailingCorner: Edge.Corner.Style
    ) -> ConcentricRectangle {
        ConcentricRectangle(uniformTopCorners: uniformTopCorners, bottomLeadingCorner: bottomLeadingCorner, bottomTrailingCorner: bottomTrailingCorner)
    }
    public static func rect(
        uniformBottomCorners: Edge.Corner.Style,
        topLeadingCorner: Edge.Corner.Style,
        topTrailingCorner: Edge.Corner.Style
    ) -> ConcentricRectangle {
        ConcentricRectangle(uniformBottomCorners: uniformBottomCorners, topLeadingCorner: topLeadingCorner, topTrailingCorner: topTrailingCorner)
    }
    public static func rect(
        uniformLeadingCorners: Edge.Corner.Style,
        topTrailingCorner: Edge.Corner.Style,
        bottomTrailingCorner: Edge.Corner.Style
    ) -> ConcentricRectangle {
        ConcentricRectangle(uniformLeadingCorners: uniformLeadingCorners, topTrailingCorner: topTrailingCorner, bottomTrailingCorner: bottomTrailingCorner)
    }
    public static func rect(
        uniformTrailingCorners: Edge.Corner.Style,
        topLeadingCorner: Edge.Corner.Style,
        bottomLeadingCorner: Edge.Corner.Style
    ) -> ConcentricRectangle {
        ConcentricRectangle(uniformTrailingCorners: uniformTrailingCorners, topLeadingCorner: topLeadingCorner, bottomLeadingCorner: bottomLeadingCorner)
    }
}

// MARK: - Shape Operations

public struct OffsetShape<Content: Shape>: Shape {
    public var shape: Content
    public var offset: CGSize
    public init(shape: Content, offset: CGSize) {
        self.shape = shape
        self.offset = offset
    }
    public static var role: ShapeRole { Content.role }
    public var layoutDirectionBehavior: LayoutDirectionBehavior { shape.layoutDirectionBehavior }
    public func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize { shape.sizeThatFits(proposal) }
    public func path(in rect: CGRect) -> Path {
        shape.path(in: rect).offsetBy(dx: offset.width, dy: offset.height)
    }
}
extension OffsetShape: InsettableShape where Content: InsettableShape {
    public func inset(by amount: CGFloat) -> OffsetShape<Content.InsetShape> {
        OffsetShape<Content.InsetShape>(shape: shape.inset(by: amount), offset: offset)
    }
}

public struct RotatedShape<Content: Shape>: Shape {
    public var shape: Content
    public var angle: Angle
    public var anchor: UnitPoint
    public init(shape: Content, angle: Angle, anchor: UnitPoint = .center) {
        self.shape = shape
        self.angle = angle
        self.anchor = anchor
    }
    public static var role: ShapeRole { Content.role }
    public var layoutDirectionBehavior: LayoutDirectionBehavior { shape.layoutDirectionBehavior }
    public func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize { shape.sizeThatFits(proposal) }
    public func path(in rect: CGRect) -> Path {
        let ax = rect.minX + rect.width * anchor.x
        let ay = rect.minY + rect.height * anchor.y
        let t = CGAffineTransform(translationX: -ax, y: -ay)
            .concatenating(CGAffineTransform(rotationAngle: CGFloat(angle.radians)))
            .concatenating(CGAffineTransform(translationX: ax, y: ay))
        return shape.path(in: rect).applying(t)
    }
}
extension RotatedShape: InsettableShape where Content: InsettableShape {
    public func inset(by amount: CGFloat) -> RotatedShape<Content.InsetShape> {
        RotatedShape<Content.InsetShape>(shape: shape.inset(by: amount), angle: angle, anchor: anchor)
    }
}

public struct ScaledShape<Content: Shape>: Shape {
    public var shape: Content
    public var scale: CGSize
    public var anchor: UnitPoint
    public init(shape: Content, scale: CGSize, anchor: UnitPoint = .center) {
        self.shape = shape
        self.scale = scale
        self.anchor = anchor
    }
    public static var role: ShapeRole { Content.role }
    public var layoutDirectionBehavior: LayoutDirectionBehavior { shape.layoutDirectionBehavior }
    public func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize { shape.sizeThatFits(proposal) }
    public func path(in rect: CGRect) -> Path {
        let ax = rect.minX + rect.width * anchor.x
        let ay = rect.minY + rect.height * anchor.y
        let t = CGAffineTransform(translationX: -ax, y: -ay)
            .concatenating(CGAffineTransform(scaleX: scale.width, y: scale.height))
            .concatenating(CGAffineTransform(translationX: ax, y: ay))
        return shape.path(in: rect).applying(t)
    }
}

public struct TransformedShape<Content: Shape>: Shape {
    public var shape: Content
    public var transform: CGAffineTransform
    public init(shape: Content, transform: CGAffineTransform) {
        self.shape = shape
        self.transform = transform
    }
    public static var role: ShapeRole { Content.role }
    public var layoutDirectionBehavior: LayoutDirectionBehavior { shape.layoutDirectionBehavior }
    public func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize { shape.sizeThatFits(proposal) }
    public func path(in rect: CGRect) -> Path {
        shape.path(in: rect).applying(transform)
    }
}

public struct _TrimmedShape<Content: Shape>: Shape {
    let shape: Content
    let startFraction: CGFloat
    let endFraction: CGFloat
    public static var role: ShapeRole { Content.role }
    public var layoutDirectionBehavior: LayoutDirectionBehavior { shape.layoutDirectionBehavior }
    public func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize { shape.sizeThatFits(proposal) }
    public func path(in rect: CGRect) -> Path {
        shape.path(in: rect).trimmedPath(from: startFraction, to: endFraction)
    }
}

public struct _SizedShape<Content: Shape>: Shape {
    let shape: Content
    let size: CGSize
    let anchor: UnitPoint
    public static var role: ShapeRole { Content.role }
    public var layoutDirectionBehavior: LayoutDirectionBehavior { shape.layoutDirectionBehavior }
    public func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize { shape.sizeThatFits(proposal) }
    public func path(in rect: CGRect) -> Path {
        let ox = rect.minX + (rect.width - size.width) * anchor.x
        let oy = rect.minY + (rect.height - size.height) * anchor.y
        return shape.path(in: CGRect(x: ox, y: oy, width: size.width, height: size.height))
    }
}

public struct _StrokedShape<Content: Shape>: Shape {
    let shape: Content
    let style: StrokeStyle
    public static var role: ShapeRole { .stroke }
    public var layoutDirectionBehavior: LayoutDirectionBehavior { shape.layoutDirectionBehavior }
    public func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize { shape.sizeThatFits(proposal) }
    public func path(in rect: CGRect) -> Path {
        shape.path(in: rect).strokedPath(style)
    }
}

public struct _BooleanShape<LHS: Shape, RHS: Shape>: Shape {
    enum Kind { case union, intersection, subtracting, symmetricDifference, lineIntersection, lineSubtraction }
    let lhs: LHS, rhs: RHS, kind: Kind, eoFill: Bool
    public var layoutDirectionBehavior: LayoutDirectionBehavior { lhs.layoutDirectionBehavior }
    public func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize { lhs.sizeThatFits(proposal) }
    public func path(in rect: CGRect) -> Path {
        let a = lhs.path(in: rect), b = rhs.path(in: rect)
        switch kind {
        case .union: return a.union(b, eoFill: eoFill)
        case .intersection: return a.intersection(b, eoFill: eoFill)
        case .subtracting: return a.subtracting(b, eoFill: eoFill)
        case .symmetricDifference: return a.symmetricDifference(b, eoFill: eoFill)
        case .lineIntersection: return a.lineIntersection(b, eoFill: eoFill)
        case .lineSubtraction: return a.lineSubtraction(b, eoFill: eoFill)
        }
    }
}

extension Shape {
    public func offset(_ offset: CGSize) -> OffsetShape<Self> { OffsetShape(shape: self, offset: offset) }
    public func offset(_ offset: CGPoint) -> OffsetShape<Self> { OffsetShape(shape: self, offset: CGSize(width: offset.x, height: offset.y)) }
    public func offset(x: CGFloat = 0, y: CGFloat = 0) -> OffsetShape<Self> { OffsetShape(shape: self, offset: CGSize(width: x, height: y)) }
    public func rotation(_ angle: Angle, anchor: UnitPoint = .center) -> RotatedShape<Self> { RotatedShape(shape: self, angle: angle, anchor: anchor) }
    public func scale(x: CGFloat = 1, y: CGFloat = 1, anchor: UnitPoint = .center) -> ScaledShape<Self> {
        ScaledShape(shape: self, scale: CGSize(width: x, height: y), anchor: anchor)
    }
    public func scale(_ scale: CGFloat, anchor: UnitPoint = .center) -> ScaledShape<Self> {
        ScaledShape(shape: self, scale: CGSize(width: scale, height: scale), anchor: anchor)
    }
    public func transform(_ transform: CGAffineTransform) -> TransformedShape<Self> { TransformedShape(shape: self, transform: transform) }
    public func trim(from startFraction: CGFloat = 0, to endFraction: CGFloat = 1) -> _TrimmedShape<Self> {
        _TrimmedShape(shape: self, startFraction: startFraction, endFraction: endFraction)
    }
    public func size(_ size: CGSize) -> _SizedShape<Self> { _SizedShape(shape: self, size: size, anchor: .topLeading) }
    public func size(width: CGFloat, height: CGFloat) -> _SizedShape<Self> {
        _SizedShape(shape: self, size: CGSize(width: width, height: height), anchor: .topLeading)
    }
    public func size(_ size: CGSize, anchor: UnitPoint) -> _SizedShape<Self> { _SizedShape(shape: self, size: size, anchor: anchor) }
    public func size(width: CGFloat, height: CGFloat, anchor: UnitPoint) -> _SizedShape<Self> {
        _SizedShape(shape: self, size: CGSize(width: width, height: height), anchor: anchor)
    }
    public func union<T: Shape>(_ other: T, eoFill: Bool = false) -> _BooleanShape<Self, T> {
        _BooleanShape(lhs: self, rhs: other, kind: .union, eoFill: eoFill)
    }
    public func intersection<T: Shape>(_ other: T, eoFill: Bool = false) -> _BooleanShape<Self, T> {
        _BooleanShape(lhs: self, rhs: other, kind: .intersection, eoFill: eoFill)
    }
    public func subtracting<T: Shape>(_ other: T, eoFill: Bool = false) -> _BooleanShape<Self, T> {
        _BooleanShape(lhs: self, rhs: other, kind: .subtracting, eoFill: eoFill)
    }
    public func symmetricDifference<T: Shape>(_ other: T, eoFill: Bool = false) -> _BooleanShape<Self, T> {
        _BooleanShape(lhs: self, rhs: other, kind: .symmetricDifference, eoFill: eoFill)
    }
    public func lineIntersection<T: Shape>(_ other: T, eoFill: Bool = false) -> _BooleanShape<Self, T> {
        _BooleanShape(lhs: self, rhs: other, kind: .lineIntersection, eoFill: eoFill)
    }
    public func lineSubtraction<T: Shape>(_ other: T, eoFill: Bool = false) -> _BooleanShape<Self, T> {
        _BooleanShape(lhs: self, rhs: other, kind: .lineSubtraction, eoFill: eoFill)
    }
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
    public enum TextStyle: CaseIterable, Hashable, Sendable {
        case largeTitle, title, title2, title3, headline, subheadline, body, callout, footnote, caption, caption2
        /// Point sizes for xSmall...accessibility5 (Apple's iOS table; same as `TextStyle::size_at` in onesim-protocol).
        var sizes: [Double] {
            switch self {
            case .largeTitle: return [31, 32, 33, 34, 36, 38, 40, 44, 48, 52, 56, 60]
            case .title: return [25, 26, 27, 28, 30, 32, 34, 38, 43, 48, 53, 58]
            case .title2: return [19, 20, 21, 22, 24, 26, 28, 34, 39, 44, 50, 56]
            case .title3: return [17, 18, 19, 20, 22, 24, 26, 31, 37, 43, 49, 55]
            case .headline, .body: return [14, 15, 16, 17, 19, 21, 23, 28, 33, 40, 47, 53]
            case .callout: return [13, 14, 15, 16, 18, 20, 22, 26, 32, 38, 44, 51]
            case .subheadline: return [12, 13, 14, 15, 17, 19, 21, 25, 30, 36, 42, 49]
            case .footnote: return [12, 12, 12, 13, 15, 17, 19, 23, 27, 33, 38, 44]
            case .caption: return [11, 11, 11, 12, 14, 16, 18, 22, 26, 32, 37, 43]
            case .caption2: return [11, 11, 11, 11, 13, 15, 17, 20, 24, 29, 34, 40]
            }
        }
    }
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

/// Built-in styles are wire names (`_styleName`); a custom style's `makeBody` replaces the button (section Q).
public protocol PrimitiveButtonStyle {
    associatedtype Body: View
    typealias Configuration = PrimitiveButtonStyleConfiguration
    @ViewBuilder func makeBody(configuration: Configuration) -> Body
    var _styleName: String? { get }
}
extension PrimitiveButtonStyle { public var _styleName: String? { nil } }
public struct DefaultButtonStyle: PrimitiveButtonStyle {
    public init() {}
    public var _styleName: String? { "automatic" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
public struct PlainButtonStyle: PrimitiveButtonStyle {
    public init() {}
    public var _styleName: String? { "plain" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
public struct BorderedButtonStyle: PrimitiveButtonStyle {
    public init() {}
    public var _styleName: String? { "bordered" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
public struct BorderedProminentButtonStyle: PrimitiveButtonStyle {
    public init() {}
    public var _styleName: String? { "borderedProminent" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
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
        let saved = (rt.disabled, rt.buttonStyle, rt.pickerStyle, rt.styles, rt.envObjects, rt.envLocal)
        apply(rt)
        defer { (rt.disabled, rt.buttonStyle, rt.pickerStyle, rt.styles, rt.envObjects, rt.envLocal) = saved }
        #else
        let saved = (rt.disabled, rt.buttonStyle, rt.pickerStyle, rt.styles, rt.imageSession, rt.envObjects, rt.envLocal)
        apply(rt)
        defer { (rt.disabled, rt.buttonStyle, rt.pickerStyle, rt.styles, rt.imageSession, rt.envObjects, rt.envLocal) = saved }
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
        _CtxView(content: self) { rt in
            if let name = style._styleName { rt.buttonStyle = name; rt.styles.button = nil }
            else { rt.styles.button = .primitive { cfg, p in _eval(style.makeBody(configuration: cfg), p) } }
        }
    }
    public func disabled(_ disabled: Bool) -> some View {
        _CtxView(content: self) { $0.disabled = $0.disabled || disabled }
    }
    public func containerShape<T: InsettableShape>(_ shape: T) -> some View {
        _modified {
            let spec = (shape as? _DeclarativeShape)?._containerSpec() ?? ["shape": "rectangle"]
            return ["containerShape": spec]
        }
    }
}

func _lineStyle(_ active: Bool, _ pattern: Text.LineStyle.Pattern, _ color: Color?) -> Node {
    var box: Node = ["active": active, "pattern": pattern._wire]
    if let color { box["color"] = color.json }
    return box
}
