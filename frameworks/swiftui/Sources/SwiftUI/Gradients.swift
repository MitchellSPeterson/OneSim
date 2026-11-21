#if !os(WASI)
import Foundation
#else
import WASILibc
#endif

public struct Gradient: Hashable, ShapeStyle {
    public struct Stop: Hashable {
        public var color: Color
        public var location: CGFloat

        public init(color: Color, location: CGFloat) {
            self.color = color
            self.location = location
        }

        public static func == (lhs: Self, rhs: Self) -> Bool {
            lhs.location == rhs.location && lhs.color.paintIdentity == rhs.color.paintIdentity
        }

        public func hash(into hasher: inout Hasher) {
            hasher.combine(location)
            hasher.combine(color.paintIdentity)
        }
    }

    public enum ColorSpace: String, Hashable, Sendable {
        case device, perceptual
    }

    public var stops: [Stop]

    public init(stops: [Stop]) { self.stops = stops }

    public init(colors: [Color]) {
        let intervals = CGFloat(max(1, colors.count - 1))
        stops = colors.enumerated().map { Stop(color: $0.element, location: CGFloat($0.offset) / intervals) }
    }

    public func colorSpace(_ space: ColorSpace) -> AnyGradient { AnyGradient(self, space: space) }
    public var _json: [String: Any] { AnyGradient(self)._json }
}

extension Color {
    fileprivate var paintIdentity: [String] {
        json.keys.sorted().map { key in
            key + "=" + String(reflecting: json[key]!)
        }
    }

    public var gradient: AnyGradient {
        let blue = json["b"] as? Double
        guard let red = json["r"] as? Double, let green = json["g"] as? Double, let blue else {
            return AnyGradient(Gradient(colors: [self]))
        }
        let alpha = json["a"] as? Double ?? 1
        let lighter = Color(red: red * 0.88 + 0.12, green: green * 0.88 + 0.12, blue: blue * 0.88 + 0.12, opacity: alpha)
        let darker = Color(red: red * 0.86, green: green * 0.86, blue: blue * 0.86, opacity: alpha)
        return AnyGradient(Gradient(colors: [lighter, darker]))
    }

    public struct Resolved: Equatable, Sendable {
        public var linearRed, linearGreen, linearBlue, opacity: Float

        public init(linearRed: Float, linearGreen: Float, linearBlue: Float, opacity: Float) {
            self.linearRed = linearRed
            self.linearGreen = linearGreen
            self.linearBlue = linearBlue
            self.opacity = opacity
        }

        public init(red: Float, green: Float, blue: Float, opacity: Float) {
            self.init(linearRed: Self.linear(red), linearGreen: Self.linear(green),
                      linearBlue: Self.linear(blue), opacity: opacity)
        }

        private static func linear(_ channel: Float) -> Float {
            let magnitude = abs(channel)
            let result = magnitude <= 0.04045 ? magnitude / 12.92 : powf((magnitude + 0.055) / 1.055, 2.4)
            return channel < 0 ? -result : result
        }

        private static func srgb(_ channel: Float) -> Float {
            let magnitude = abs(channel)
            let result = magnitude <= 0.0031308 ? magnitude * 12.92 : 1.055 * powf(magnitude, 1 / 2.4) - 0.055
            return channel < 0 ? -result : result
        }

        public var red: Float { Self.srgb(linearRed) }
        public var green: Float { Self.srgb(linearGreen) }
        public var blue: Float { Self.srgb(linearBlue) }

        var color: Color {
            Color(red: Double(red), green: Double(green), blue: Double(blue), opacity: Double(opacity))
        }
    }
}

public struct AnyGradient: Hashable, ShapeStyle {
    let gradient: Gradient
    let space: Gradient.ColorSpace

    public init(_ gradient: Gradient) { self.init(gradient, space: .device) }

    init(_ gradient: Gradient, space: Gradient.ColorSpace) {
        self.gradient = gradient
        self.space = space
    }

    func paint(_ kind: String, _ geometry: Node) -> Node {
        var fields: Node = geometry.mapValues { value in
            if let pair = value as? [Double] { return pair.map { $0.isFinite ? $0 : 0 } }
            if let number = value as? Double { return number.isFinite ? number : 0 }
            return value
        }
        fields["kind"] = kind
        fields["colorSpace"] = space.rawValue
        fields["stops"] = gradient.stops.filter { $0.location.isFinite }.map { stop in
            ["location": Double(stop.location), "color": stop.color.json] as Node
        }
        return ["gradient": fields]
    }

    public var _json: [String: Any] { paint("linear", ["start": [0.5, 0.0], "end": [0.5, 1.0]]) }
}

public struct AnyShapeStyle: ShapeStyle {
    private let style: any ShapeStyle
    public init<S: ShapeStyle>(_ style: S) { self.style = style }
    public var _json: [String: Any] { style._json }
}

protocol _GradientView: ShapeStyle, View, _Primitive {}
extension _GradientView {
    func _emit(_ path: String) -> Node {
        ["id": path, "kind": "shape", "shape": "rectangle", "fill": _json]
    }
}

extension UnitPoint {
    var paintPoint: [Double] { [Double(x), Double(y)] }
}

public struct LinearGradient: _GradientView {
    public typealias Body = Never
    public var body: Never { fatalError() }
    public let _json: [String: Any]

    init(_ gradient: AnyGradient, startPoint: UnitPoint, endPoint: UnitPoint) {
        _json = gradient.paint("linear", [
            "start": startPoint.paintPoint, "end": endPoint.paintPoint,
        ])
    }

    public init(gradient: Gradient, startPoint: UnitPoint, endPoint: UnitPoint) {
        self.init(AnyGradient(gradient), startPoint: startPoint, endPoint: endPoint)
    }

    public init(colors: [Color], startPoint: UnitPoint, endPoint: UnitPoint) {
        self.init(gradient: Gradient(colors: colors), startPoint: startPoint, endPoint: endPoint)
    }

    public init(stops: [Gradient.Stop], startPoint: UnitPoint, endPoint: UnitPoint) {
        self.init(gradient: Gradient(stops: stops), startPoint: startPoint, endPoint: endPoint)
    }
}

extension ShapeStyle where Self == LinearGradient {
    public static func linearGradient(_ gradient: Gradient, startPoint: UnitPoint, endPoint: UnitPoint) -> LinearGradient {
        LinearGradient(gradient: gradient, startPoint: startPoint, endPoint: endPoint)
    }

    public static func linearGradient(colors: [Color], startPoint: UnitPoint, endPoint: UnitPoint) -> LinearGradient {
        LinearGradient(colors: colors, startPoint: startPoint, endPoint: endPoint)
    }

    public static func linearGradient(stops: [Gradient.Stop], startPoint: UnitPoint, endPoint: UnitPoint) -> LinearGradient {
        LinearGradient(stops: stops, startPoint: startPoint, endPoint: endPoint)
    }

    public static func linearGradient(_ gradient: AnyGradient, startPoint: UnitPoint, endPoint: UnitPoint) -> some ShapeStyle {
        LinearGradient(gradient, startPoint: startPoint, endPoint: endPoint)
    }
}

public struct RadialGradient: _GradientView {
    public typealias Body = Never
    public var body: Never { fatalError() }
    public let _json: [String: Any]

    init(_ gradient: AnyGradient, center: UnitPoint, startRadius: CGFloat, endRadius: CGFloat) {
        _json = gradient.paint("radial", [
            "center": center.paintPoint, "startRadius": Double(startRadius),
            "endRadius": Double(endRadius),
        ])
    }

    public init(gradient: Gradient, center: UnitPoint, startRadius: CGFloat, endRadius: CGFloat) {
        self.init(AnyGradient(gradient), center: center, startRadius: startRadius, endRadius: endRadius)
    }

    public init(colors: [Color], center: UnitPoint, startRadius: CGFloat, endRadius: CGFloat) {
        self.init(gradient: Gradient(colors: colors), center: center, startRadius: startRadius, endRadius: endRadius)
    }

    public init(stops: [Gradient.Stop], center: UnitPoint, startRadius: CGFloat, endRadius: CGFloat) {
        self.init(gradient: Gradient(stops: stops), center: center, startRadius: startRadius, endRadius: endRadius)
    }
}

extension ShapeStyle where Self == RadialGradient {
    public static func radialGradient(_ gradient: Gradient, center: UnitPoint, startRadius: CGFloat, endRadius: CGFloat) -> RadialGradient {
        RadialGradient(gradient: gradient, center: center, startRadius: startRadius, endRadius: endRadius)
    }

    public static func radialGradient(colors: [Color], center: UnitPoint, startRadius: CGFloat, endRadius: CGFloat) -> RadialGradient {
        RadialGradient(colors: colors, center: center, startRadius: startRadius, endRadius: endRadius)
    }

    public static func radialGradient(stops: [Gradient.Stop], center: UnitPoint, startRadius: CGFloat, endRadius: CGFloat) -> RadialGradient {
        RadialGradient(stops: stops, center: center, startRadius: startRadius, endRadius: endRadius)
    }

    public static func radialGradient(_ gradient: AnyGradient, center: UnitPoint = .center, startRadius: CGFloat = 0, endRadius: CGFloat) -> some ShapeStyle {
        RadialGradient(gradient, center: center, startRadius: startRadius, endRadius: endRadius)
    }
}

public struct EllipticalGradient: _GradientView {
    public typealias Body = Never
    public var body: Never { fatalError() }
    public let _json: [String: Any]

    init(_ gradient: AnyGradient, center: UnitPoint = .center, startRadiusFraction: CGFloat = 0, endRadiusFraction: CGFloat = 0.5) {
        _json = gradient.paint("radial", [
            "center": center.paintPoint, "startRadius": Double(startRadiusFraction),
            "endRadius": Double(endRadiusFraction), "elliptical": true,
        ])
    }

    public init(gradient: Gradient, center: UnitPoint = .center, startRadiusFraction: CGFloat = 0, endRadiusFraction: CGFloat = 0.5) {
        self.init(AnyGradient(gradient), center: center, startRadiusFraction: startRadiusFraction, endRadiusFraction: endRadiusFraction)
    }

    public init(colors: [Color], center: UnitPoint = .center, startRadiusFraction: CGFloat = 0, endRadiusFraction: CGFloat = 0.5) {
        self.init(gradient: Gradient(colors: colors), center: center, startRadiusFraction: startRadiusFraction, endRadiusFraction: endRadiusFraction)
    }

    public init(stops: [Gradient.Stop], center: UnitPoint = .center, startRadiusFraction: CGFloat = 0, endRadiusFraction: CGFloat = 0.5) {
        self.init(gradient: Gradient(stops: stops), center: center, startRadiusFraction: startRadiusFraction, endRadiusFraction: endRadiusFraction)
    }
}

extension ShapeStyle where Self == EllipticalGradient {
    public static func ellipticalGradient(_ gradient: Gradient, center: UnitPoint = .center, startRadiusFraction: CGFloat = 0, endRadiusFraction: CGFloat = 0.5) -> EllipticalGradient {
        EllipticalGradient(gradient: gradient, center: center, startRadiusFraction: startRadiusFraction, endRadiusFraction: endRadiusFraction)
    }

    public static func ellipticalGradient(colors: [Color], center: UnitPoint = .center, startRadiusFraction: CGFloat = 0, endRadiusFraction: CGFloat = 0.5) -> EllipticalGradient {
        EllipticalGradient(colors: colors, center: center, startRadiusFraction: startRadiusFraction, endRadiusFraction: endRadiusFraction)
    }

    public static func ellipticalGradient(stops: [Gradient.Stop], center: UnitPoint = .center, startRadiusFraction: CGFloat = 0, endRadiusFraction: CGFloat = 0.5) -> EllipticalGradient {
        EllipticalGradient(stops: stops, center: center, startRadiusFraction: startRadiusFraction, endRadiusFraction: endRadiusFraction)
    }

    public static func ellipticalGradient(_ gradient: AnyGradient, center: UnitPoint = .center, startRadiusFraction: CGFloat = 0, endRadiusFraction: CGFloat = 0.5) -> some ShapeStyle {
        EllipticalGradient(gradient, center: center, startRadiusFraction: startRadiusFraction, endRadiusFraction: endRadiusFraction)
    }
}

public struct AngularGradient: _GradientView {
    public typealias Body = Never
    public var body: Never { fatalError() }
    public let _json: [String: Any]

    init(_ gradient: AnyGradient, center: UnitPoint, startAngle: Angle, endAngle: Angle) {
        _json = gradient.paint("angular", [
            "center": center.paintPoint, "startAngle": startAngle.radians, "endAngle": endAngle.radians,
        ])
    }

    public init(gradient: Gradient, center: UnitPoint, startAngle: Angle, endAngle: Angle) {
        self.init(AnyGradient(gradient), center: center, startAngle: startAngle, endAngle: endAngle)
    }

    public init(colors: [Color], center: UnitPoint, startAngle: Angle, endAngle: Angle) {
        self.init(gradient: Gradient(colors: colors), center: center, startAngle: startAngle, endAngle: endAngle)
    }

    public init(stops: [Gradient.Stop], center: UnitPoint, startAngle: Angle, endAngle: Angle) {
        self.init(gradient: Gradient(stops: stops), center: center, startAngle: startAngle, endAngle: endAngle)
    }

    public init(gradient: Gradient, center: UnitPoint, angle: Angle = .zero) {
        self.init(gradient: gradient, center: center, startAngle: angle, endAngle: .radians(angle.radians + 2 * Double.pi))
    }

    public init(colors: [Color], center: UnitPoint, angle: Angle = .zero) {
        self.init(colors: colors, center: center, startAngle: angle, endAngle: .radians(angle.radians + 2 * Double.pi))
    }

    public init(stops: [Gradient.Stop], center: UnitPoint, angle: Angle = .zero) {
        self.init(stops: stops, center: center, startAngle: angle, endAngle: .radians(angle.radians + 2 * Double.pi))
    }
}

extension ShapeStyle where Self == AngularGradient {
    public static func angularGradient(_ gradient: Gradient, center: UnitPoint, startAngle: Angle, endAngle: Angle) -> AngularGradient {
        AngularGradient(gradient: gradient, center: center, startAngle: startAngle, endAngle: endAngle)
    }

    public static func angularGradient(colors: [Color], center: UnitPoint, startAngle: Angle, endAngle: Angle) -> AngularGradient {
        AngularGradient(colors: colors, center: center, startAngle: startAngle, endAngle: endAngle)
    }

    public static func angularGradient(stops: [Gradient.Stop], center: UnitPoint, startAngle: Angle, endAngle: Angle) -> AngularGradient {
        AngularGradient(stops: stops, center: center, startAngle: startAngle, endAngle: endAngle)
    }

    public static func angularGradient(_ gradient: AnyGradient, center: UnitPoint = .center, startAngle: Angle, endAngle: Angle) -> some ShapeStyle {
        AngularGradient(gradient, center: center, startAngle: startAngle, endAngle: endAngle)
    }

    public static func conicGradient(_ gradient: Gradient, center: UnitPoint, angle: Angle = .zero) -> AngularGradient {
        AngularGradient(gradient: gradient, center: center, angle: angle)
    }

    public static func conicGradient(colors: [Color], center: UnitPoint, angle: Angle = .zero) -> AngularGradient {
        AngularGradient(colors: colors, center: center, angle: angle)
    }

    public static func conicGradient(stops: [Gradient.Stop], center: UnitPoint, angle: Angle = .zero) -> AngularGradient {
        AngularGradient(stops: stops, center: center, angle: angle)
    }

    public static func conicGradient(_ gradient: AnyGradient, center: UnitPoint = .center, angle: Angle = .zero) -> some ShapeStyle {
        AngularGradient(gradient, center: center, startAngle: angle, endAngle: .radians(angle.radians + 2 * Double.pi))
    }
}


public struct MeshGradient: _GradientView, Equatable {
    public typealias Body = Never
    public var body: Never { fatalError() }

    public struct BezierPoint: Equatable, Sendable {
        public var position, leadingControlPoint, topControlPoint, trailingControlPoint, bottomControlPoint: SIMD2<Float>

        public init(position: SIMD2<Float>, leadingControlPoint: SIMD2<Float>, topControlPoint: SIMD2<Float>,
                    trailingControlPoint: SIMD2<Float>, bottomControlPoint: SIMD2<Float>) {
            self.position = position
            self.leadingControlPoint = leadingControlPoint
            self.topControlPoint = topControlPoint
            self.trailingControlPoint = trailingControlPoint
            self.bottomControlPoint = bottomControlPoint
        }

        fileprivate var json: Node {
            ["position": _meshPoint(position), "controls": [
                leadingControlPoint, topControlPoint, trailingControlPoint, bottomControlPoint,
            ].map(_meshPoint)]
        }
    }

    public enum Locations: Equatable, Sendable {
        case points([SIMD2<Float>]), bezierPoints([BezierPoint])

        fileprivate var json: [Node] {
            switch self {
            case .points(let points): return points.map { ["position": _meshPoint($0)] }
            case .bezierPoints(let points): return points.map { $0.json }
            }
        }
    }

    public enum Colors: Equatable {
        case colors([Color]), resolvedColors([Color.Resolved])

        fileprivate var values: [Color] {
            switch self {
            case .colors(let colors): return colors
            case .resolvedColors(let colors): return colors.map { $0.color }
            }
        }

        public static func == (lhs: Self, rhs: Self) -> Bool {
            switch (lhs, rhs) {
            case (.colors(let l), .colors(let r)): return l.map { $0.paintIdentity } == r.map { $0.paintIdentity }
            case (.resolvedColors(let l), .resolvedColors(let r)): return l == r
            default: return false
            }
        }
    }

    public var width, height: Int
    public var locations: Locations
    public var colors: Colors
    public var background: Color
    public var smoothsColors: Bool
    public var colorSpace: Gradient.ColorSpace

    public init(width: Int, height: Int, locations: Locations, colors: Colors, background: Color = .clear,
                smoothsColors: Bool = true, colorSpace: Gradient.ColorSpace = .device) {
        self.width = width
        self.height = height
        self.locations = locations
        self.colors = colors
        self.background = background
        self.smoothsColors = smoothsColors
        self.colorSpace = colorSpace
    }

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.width == rhs.width && lhs.height == rhs.height && lhs.locations == rhs.locations &&
        lhs.colors == rhs.colors && lhs.background.paintIdentity == rhs.background.paintIdentity &&
        lhs.smoothsColors == rhs.smoothsColors && lhs.colorSpace == rhs.colorSpace
    }

    public var _json: [String: Any] {
        ["gradient": [
            "kind": "mesh", "width": max(0, width), "height": max(0, height),
            "points": locations.json, "colors": colors.values.map { $0.json },
            "background": background.json, "smoothsColors": smoothsColors, "colorSpace": colorSpace.rawValue,
        ] as Node]
    }

    public init(width: Int, height: Int, points: [SIMD2<Float>], colors: [Color], background: Color = .clear,
                smoothsColors: Bool = true, colorSpace: Gradient.ColorSpace = .device) {
        self.init(width: width, height: height, locations: .points(points), colors: .colors(colors),
                  background: background, smoothsColors: smoothsColors, colorSpace: colorSpace)
    }

    public init(width: Int, height: Int, points: [SIMD2<Float>], resolvedColors: [Color.Resolved], background: Color = .clear,
                smoothsColors: Bool = true, colorSpace: Gradient.ColorSpace = .device) {
        self.init(width: width, height: height, locations: .points(points), colors: .resolvedColors(resolvedColors),
                  background: background, smoothsColors: smoothsColors, colorSpace: colorSpace)
    }

    public init(width: Int, height: Int, bezierPoints: [BezierPoint], colors: [Color], background: Color = .clear,
                smoothsColors: Bool = true, colorSpace: Gradient.ColorSpace = .device) {
        self.init(width: width, height: height, locations: .bezierPoints(bezierPoints), colors: .colors(colors),
                  background: background, smoothsColors: smoothsColors, colorSpace: colorSpace)
    }

    public init(width: Int, height: Int, bezierPoints: [BezierPoint], resolvedColors: [Color.Resolved], background: Color = .clear,
                smoothsColors: Bool = true, colorSpace: Gradient.ColorSpace = .device) {
        self.init(width: width, height: height, locations: .bezierPoints(bezierPoints), colors: .resolvedColors(resolvedColors),
                  background: background, smoothsColors: smoothsColors, colorSpace: colorSpace)
    }
}

private func _meshPoint(_ point: SIMD2<Float>) -> [Double] {
    [Double(point.x), Double(point.y)].map { $0.isFinite ? $0 : 0 }
}
