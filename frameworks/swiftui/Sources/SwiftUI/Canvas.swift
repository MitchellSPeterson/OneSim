#if !os(WASI)
import Foundation
#endif
#if canImport(CoreGraphics)
import CoreGraphics
#endif

private final class _CanvasLayer {
    var commands: [Node] = []
    var inkBounds = CGRect.null
}

public struct Canvas<Symbols: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    public var symbols: Symbols
    public var renderer: (inout GraphicsContext, CGSize) -> Void
    public var isOpaque: Bool
    public var colorMode: ColorRenderingMode
    public var rendersAsynchronously: Bool

    public init(opaque: Bool = false, colorMode: ColorRenderingMode = .nonLinear,
                rendersAsynchronously: Bool = false, renderer: @escaping (inout GraphicsContext, CGSize) -> Void,
                @ViewBuilder symbols: () -> Symbols) {
        self.isOpaque = opaque
        self.colorMode = colorMode
        self.rendersAsynchronously = rendersAsynchronously
        self.renderer = renderer
        self.symbols = symbols()
    }

    func _emit(_ path: String) -> Node {
        let runtime = _Runtime.shared
        var result: Node = ["kind": "canvas", "id": path, "children": [], "opaque": isOpaque, "colorMode": colorMode.rawValue]
        guard let size = runtime.shapeSizes[path] else { return result }
        let template = _children(symbols, path + "/symbols")
        var tagged: [AnyHashable: Node] = [:]
        for node in template {
            if let key = node["id"] as? String, let tag = runtime.tags[key] { tagged[tag] = node }
        }
        var context = GraphicsContext(path: path, size: size, symbols: tagged, colorMode: colorMode)
        renderer(&context, size)
        result["proposal"] = [Double(size.width), Double(size.height)]
        result["children"] = context.commands
        return result
    }
}

extension Canvas where Symbols == EmptyView {
    public init(opaque: Bool = false, colorMode: ColorRenderingMode = .nonLinear,
                rendersAsynchronously: Bool = false, renderer: @escaping (inout GraphicsContext, CGSize) -> Void) {
        self.init(opaque: opaque, colorMode: colorMode, rendersAsynchronously: rendersAsynchronously,
                  renderer: renderer, symbols: { EmptyView() })
    }
}

public struct GraphicsContext {
    public struct BlendMode: RawRepresentable, Hashable, Sendable {
        public let rawValue: Int32
        public init(rawValue: Int32) { self.rawValue = rawValue }

        private static let names = [
            "normal", "multiply", "screen", "overlay", "darken", "lighten", "colorDodge", "colorBurn",
            "softLight", "hardLight", "difference", "exclusion", "hue", "saturation", "color", "luminosity",
            "clear", "copy", "sourceIn", "sourceOut", "sourceAtop", "destinationOver", "destinationIn",
            "destinationOut", "destinationAtop", "xor", "plusDarker", "plusLighter",
        ]
        var name: String {
            let index = Int(rawValue)
            return Self.names.indices.contains(index) ? Self.names[index] : "normal"
        }
        public static let normal = Self(rawValue: 0)
        public static let multiply = Self(rawValue: 1)
        public static let screen = Self(rawValue: 2)
        public static let overlay = Self(rawValue: 3)
        public static let darken = Self(rawValue: 4)
        public static let lighten = Self(rawValue: 5)
        public static let colorDodge = Self(rawValue: 6)
        public static let colorBurn = Self(rawValue: 7)
        public static let softLight = Self(rawValue: 8)
        public static let hardLight = Self(rawValue: 9)
        public static let difference = Self(rawValue: 10)
        public static let exclusion = Self(rawValue: 11)
        public static let hue = Self(rawValue: 12)
        public static let saturation = Self(rawValue: 13)
        public static let color = Self(rawValue: 14)
        public static let luminosity = Self(rawValue: 15)
        public static let clear = Self(rawValue: 16)
        public static let copy = Self(rawValue: 17)
        public static let sourceIn = Self(rawValue: 18)
        public static let sourceOut = Self(rawValue: 19)
        public static let sourceAtop = Self(rawValue: 20)
        public static let destinationOver = Self(rawValue: 21)
        public static let destinationIn = Self(rawValue: 22)
        public static let destinationOut = Self(rawValue: 23)
        public static let destinationAtop = Self(rawValue: 24)
        public static let xor = Self(rawValue: 25)
        public static let plusDarker = Self(rawValue: 26)
        public static let plusLighter = Self(rawValue: 27)
    }

    public struct ClipOptions: OptionSet, Sendable {
        public let rawValue: UInt32
        public init(rawValue: UInt32) { self.rawValue = rawValue }
        public static let inverse = Self(rawValue: 1)
    }

    public struct GradientOptions: OptionSet, Sendable {
        public let rawValue: UInt32
        public init(rawValue: UInt32) { self.rawValue = rawValue }
        public static let linearColor = Self(rawValue: 1)
        public static let `repeat` = Self(rawValue: 2)
        public static let mirror = Self(rawValue: 4)
    }

    public struct BlurOptions: OptionSet, Sendable {
        public let rawValue: UInt32
        public init(rawValue: UInt32) { self.rawValue = rawValue }
        public static let opaque = Self(rawValue: 1)
        public static let dithersResult = Self(rawValue: 2)
    }

    public struct FilterOptions: OptionSet, Sendable {
        public let rawValue: UInt32
        public init(rawValue: UInt32) { self.rawValue = rawValue }
        public static let linearColor = Self(rawValue: 1)
    }

    public struct Filter {
        fileprivate let json: Node
        private init(_ effect: _PaintEffect) { json = effect.json }
        private init(json: Node) { self.json = json }

        public static func projectionTransform(_ matrix: ProjectionTransform) -> Self {
            Self(.projection(matrix))
        }
        public static func blur(radius: CGFloat, options: BlurOptions = []) -> Self {
            Self(.blur(radius, opaque: options.contains(.opaque)))
        }
        public static func colorMultiply(_ color: Color) -> Self {
            Self(json: ["kind": "colorMultiply", "color": color.json])
        }
        public static func hueRotation(_ angle: Angle) -> Self { Self(.hueRotation(angle.radians)) }
        public static func saturation(_ amount: Double) -> Self { Self(.value(.saturation, amount)) }
        public static func brightness(_ amount: Double) -> Self { Self(.value(.brightness, amount)) }
        public static func contrast(_ amount: Double) -> Self { Self(.value(.contrast, amount)) }
        public static func colorInvert(_ amount: Double = 1) -> Self {
            Self(json: ["kind": "colorInvert", "amount": amount.isFinite ? amount : 0])
        }
        public static func grayscale(_ amount: Double) -> Self { Self(.value(.grayscale, amount)) }
        public static var luminanceToAlpha: Self { Self(.luminanceToAlpha) }
        public static func alphaThreshold(min: Double, max: Double = 1, color: Color = .black) -> Self {
            Self(json: ["kind": "alphaThreshold", "min": min.isFinite ? min : 0, "max": max.isFinite ? max : 1, "color": color.json])
        }
    }

    public struct Shading {
        fileprivate let json: Node
        public static var foreground: Self { style(ForegroundStyle()) }
        public static func color(_ color: Color) -> Self { Self(json: color.json) }
        public static func style<S: ShapeStyle>(_ style: S) -> Self { Self(json: style._json) }
        public static func meshGradient(_ mesh: MeshGradient) -> Self { style(mesh) }

        public static func linearGradient(_ gradient: Gradient, startPoint: CGPoint, endPoint: CGPoint,
                                          options: GradientOptions = []) -> Self {
            linearGradient(AnyGradient(gradient), startPoint: startPoint, endPoint: endPoint, options: options)
        }
        public static func linearGradient(_ gradient: AnyGradient, startPoint: CGPoint, endPoint: CGPoint,
                                          options: GradientOptions = []) -> Self {
            absolute(gradient, kind: "linear", fields: ["start": startPoint._canvasPoint, "end": endPoint._canvasPoint], options: options)
        }
        public static func radialGradient(_ gradient: Gradient, center: CGPoint, startRadius: CGFloat, endRadius: CGFloat,
                                          options: GradientOptions = []) -> Self {
            radialGradient(AnyGradient(gradient), center: center, startRadius: startRadius, endRadius: endRadius, options: options)
        }
        public static func radialGradient(_ gradient: AnyGradient, center: CGPoint, startRadius: CGFloat, endRadius: CGFloat,
                                          options: GradientOptions = []) -> Self {
            radialGradient(gradient, startCenter: center, startRadius: startRadius, endCenter: center, endRadius: endRadius, options: options)
        }
        public static func radialGradient(_ gradient: Gradient, startCenter: CGPoint, startRadius: CGFloat, endCenter: CGPoint,
                                          endRadius: CGFloat, options: GradientOptions = []) -> Self {
            radialGradient(AnyGradient(gradient), startCenter: startCenter, startRadius: startRadius, endCenter: endCenter,
                           endRadius: endRadius, options: options)
        }
        public static func radialGradient(_ gradient: AnyGradient, startCenter: CGPoint, startRadius: CGFloat, endCenter: CGPoint,
                                          endRadius: CGFloat, options: GradientOptions = []) -> Self {
            absolute(gradient, kind: "radial", fields: [
                "startCenter": startCenter._canvasPoint, "center": endCenter._canvasPoint,
                "startRadius": Double(startRadius), "endRadius": Double(endRadius),
            ], options: options)
        }
        public static func conicGradient(_ gradient: Gradient, center: CGPoint, angle: Angle = .zero,
                                         options: GradientOptions = []) -> Self {
            conicGradient(AnyGradient(gradient), center: center, angle: angle, options: options)
        }
        public static func conicGradient(_ gradient: AnyGradient, center: CGPoint, angle: Angle = .zero,
                                         options: GradientOptions = []) -> Self {
            absolute(gradient, kind: "angular", fields: [
                "center": center._canvasPoint, "startAngle": angle.radians, "endAngle": angle.radians + 2 * Double.pi,
            ], options: options)
        }

        private static func absolute(_ gradient: AnyGradient, kind: String, fields: Node, options: GradientOptions) -> Self {
            var location = fields
            location["absolute"] = true
            location["options"] = UInt64(options.rawValue)
            var result = gradient.paint(kind, location)
            if options.contains(.linearColor), var paint = result["gradient"] as? Node {
                paint["colorSpace"] = "linear"
                result["gradient"] = paint
            }
            return Self(json: result)
        }
    }

    public struct ResolvedText {
        let text: Text
        public var shading = Shading.foreground
    }
    public struct ResolvedImage {
        let image: Image
        public var shading: Shading? = nil
    }
    public struct ResolvedSymbol {
        let node: Node
    }

    public var opacity: Double = 1
    public var blendMode: BlendMode = .normal
    public var transform = CGAffineTransform.identity
    public var environment: EnvironmentValues
    private var filters: [Node] = []
    private var clips: [Node] = []
    private var tracksInk = false
    private var clipBounds: CGRect
    private var layer = _CanvasLayer()
    private let path: String
    private let size: CGSize
    private let symbols: [AnyHashable: Node]
    private let colorMode: ColorRenderingMode

    fileprivate init(path: String, size: CGSize, symbols: [AnyHashable: Node], colorMode: ColorRenderingMode) {
        self.path = path
        self.size = size
        self.symbols = symbols
        self.colorMode = colorMode
        self.clipBounds = CGRect(origin: .zero, size: size)
        self.environment = EnvironmentValues()
    }

    fileprivate var commands: [Node] { layer.commands }

    public var clipBoundingRect: CGRect {
        clipBounds.applying(transform.inverted())
    }

    public mutating func concatenate(_ matrix: CGAffineTransform) { transform = matrix.concatenating(transform) }
    public mutating func translateBy(x: CGFloat, y: CGFloat) { concatenate(CGAffineTransform(translationX: x, y: y)) }
    public mutating func scaleBy(x: CGFloat, y: CGFloat) { concatenate(CGAffineTransform(scaleX: x, y: y)) }
    public mutating func rotate(by angle: Angle) { concatenate(CGAffineTransform(rotationAngle: CGFloat(angle.radians))) }

    public mutating func addFilter(_ filter: Filter, options: FilterOptions = []) { filters.append(filter.json) }

    public mutating func clip(to path: Path, style: FillStyle = FillStyle(), options: ClipOptions = []) {
        let transformed = path.applying(transform)
        let inverse = options.contains(.inverse)
        if !inverse { clipBounds = _canvasIntersection(clipBounds, transformed.boundingRect) }
        let node = pathNode(transformed, paint: .color(.white), fill: style, stroke: nil)
        clips.append(mask(node, inverse: inverse))
    }

    public mutating func clipToLayer(opacity: Double = 1, options: ClipOptions = [],
                                     content: (inout GraphicsContext) throws -> Void) rethrows {
        var child = nested()
        child.tracksInk = true
        child.transform = transform
        try content(&child)
        let node = layerNode(child.commands, id: nextID + "/clip", modifiers: [["opacity": opacity]])
        let inverse = options.contains(.inverse)
        if !inverse {
            clipBounds = opacity > 0 ? _canvasIntersection(clipBounds, child.layer.inkBounds) : .null
        }
        clips.append(mask(node, inverse: inverse))
    }

    public func drawLayer(content: (inout GraphicsContext) throws -> Void) rethrows {
        var child = nested()
        try content(&child)
        record(layerNode(child.commands, id: nextID + "/layer"), bounds: child.layer.inkBounds)
    }

    public func fill(_ path: Path, with shading: Shading, style: FillStyle = FillStyle()) {
        record(pathNode(path, paint: shading, fill: style, stroke: nil), bounds: tracksInk ? path.boundingRect : nil)
    }
    public func stroke(_ path: Path, with shading: Shading, style: StrokeStyle) {
        record(pathNode(path, paint: shading, fill: nil, stroke: style),
               bounds: tracksInk ? path.strokedPath(style).boundingRect : nil)
    }
    public func stroke(_ path: Path, with shading: Shading, lineWidth: CGFloat = 1) {
        stroke(path, with: shading, style: StrokeStyle(lineWidth: lineWidth))
    }

    public func resolve(_ text: Text) -> ResolvedText { ResolvedText(text: text) }
    public func resolve(_ image: Image) -> ResolvedImage { ResolvedImage(image: image) }
    public func resolveSymbol<ID: Hashable>(id: ID) -> ResolvedSymbol? {
        symbols[AnyHashable(id)].map { ResolvedSymbol(node: $0) }
    }
    public func draw(_ text: Text, at point: CGPoint, anchor: UnitPoint = .center) {
        draw(resolve(text), at: point, anchor: anchor)
    }
    public func draw(_ text: Text, in rect: CGRect) { draw(resolve(text), in: rect) }
    public func draw(_ text: ResolvedText, at point: CGPoint, anchor: UnitPoint = .center) {
        place(resolvedTextNode(text), at: point, anchor: anchor)
    }
    public func draw(_ text: ResolvedText, in rect: CGRect) {
        place(resolvedTextNode(text), in: rect)
    }
    public func draw(_ image: Image, at point: CGPoint, anchor: UnitPoint = .center) {
        draw(resolve(image), at: point, anchor: anchor)
    }
    public func draw(_ image: Image, in rect: CGRect) { draw(resolve(image), in: rect) }
    public func draw(_ image: ResolvedImage, at point: CGPoint, anchor: UnitPoint = .center) {
        place(resolvedImageNode(image), at: point, anchor: anchor)
    }
    public func draw(_ image: ResolvedImage, in rect: CGRect) {
        place(resolvedImageNode(image), in: rect)
    }
    public func draw(_ symbol: ResolvedSymbol, at point: CGPoint, anchor: UnitPoint = .center) {
        place(symbol.node, at: point, anchor: anchor)
    }
    public func draw(_ symbol: ResolvedSymbol, in rect: CGRect) { place(symbol.node, in: rect) }

    private func resolvedTextNode(_ text: ResolvedText) -> Node {
        let (key, paint) = _foregroundPaint(text.shading.json)
        var resolved = text.text
        for i in resolved.runs.indices {
            if resolved.runs[i].attrs["foregroundColor"] == nil && resolved.runs[i].attrs["foregroundPaint"] == nil {
                resolved.runs[i].attrs[key] = paint
            }
        }
        return _eval(resolved, nextID + "/text")
    }
    private func resolvedImageNode(_ image: ResolvedImage) -> Node {
        if let shading = image.shading {
            return _eval(image.image.foregroundStyle(_CanvasPaint(json: shading.json)), nextID + "/image")
        }
        return _eval(image.image, nextID + "/image")
    }
    private func place(_ child: Node, at point: CGPoint, anchor: UnitPoint) {
        record(["id": nextID, "kind": "canvasItem", "child": child, "origin": point._canvasPoint, "anchor": anchor.paintPoint])
    }
    private func place(_ child: Node, in rect: CGRect) {
        record(["id": nextID, "kind": "canvasItem", "child": child, "origin": rect.origin._canvasPoint,
                "anchor": [0.0, 0.0], "size": [Double(rect.width), Double(rect.height)]], bounds: rect)
    }

    private var nextID: String { path + "/draw" + String(layer.commands.count) }

    private func nested() -> Self {
        var context = GraphicsContext(path: nextID, size: size, symbols: symbols, colorMode: colorMode)
        context.environment = environment
        context.tracksInk = tracksInk
        return context
    }

    private func pathNode(_ path: Path, paint: Shading, fill: FillStyle?, stroke: StrokeStyle?) -> Node {
        var node: Node = ["id": nextID, "kind": "shape", "shape": "path", "path": path._wireOps()]
        if !paint.json.isEmpty && stroke == nil { node["fill"] = paint.json }
        if let fill = fill?._json { node["fillStyle"] = fill }
        if let stroke {
            node["stroke"] = stroke._json(color: paint.json.isEmpty ? nil : paint.json, antialiased: true)
        }
        return node
    }

    private func layerNode(_ children: [Node], id: String, modifiers: [Node] = []) -> Node {
        ["id": id, "kind": "canvas", "proposal": [Double(size.width), Double(size.height)], "children": children,
         "modifiers": modifiers, "opaque": false]
    }

    private func mask(_ node: Node, inverse: Bool) -> Node {
        [inverse ? "invertedMask" : "mask": ["view": node]]
    }

    private func record(_ node: Node, bounds: CGRect? = nil) {
        var node = node
        var mods = node["modifiers"] as? [Node] ?? []
        if !transform.isIdentity { mods.append(["effect": _PaintEffect.affine(transform, anchor: .zero).json]) }
        mods += filters.map { ["effect": $0] }
        if opacity != 1 { mods.append(["effect": _PaintEffect.value(.opacity, opacity).json]) }
        mods += clips
        if blendMode != .normal { mods.append(["effect": _PaintEffect.blend(blendMode.name).json]) }
        if !mods.isEmpty { node["modifiers"] = mods }
        layer.commands.append(node)
        if tracksInk && opacity > 0 {
            let transformed = bounds.map { $0.applying(transform) } ?? clipBounds
            let ink = _canvasIntersection(transformed, clipBounds)
            layer.inkBounds = _canvasUnion(layer.inkBounds, ink)
        }
    }
}

private struct _CanvasPaint: ShapeStyle {
    let json: Node
    var _json: [String: Any] { json }
}

private extension CGPoint {
    var _canvasPoint: [Double] { [Double(x), Double(y)].map { $0.isFinite ? $0 : 0 } }
}


private func _canvasIntersection(_ a: CGRect, _ b: CGRect) -> CGRect {
    guard !a.isNull, !b.isNull else { return .null }
    let x = max(a.minX, b.minX), y = max(a.minY, b.minY)
    let width = min(a.maxX, b.maxX) - x, height = min(a.maxY, b.maxY) - y
    guard width >= 0, height >= 0 else { return .null }
    return CGRect(x: x, y: y, width: width, height: height)
}

private func _canvasUnion(_ a: CGRect, _ b: CGRect) -> CGRect {
    if a.isNull { return b }
    if b.isNull { return a }
    let x = min(a.minX, b.minX), y = min(a.minY, b.minY)
    return CGRect(x: x, y: y, width: max(a.maxX, b.maxX) - x, height: max(a.maxY, b.maxY) - y)
}
