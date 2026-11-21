#if os(WASI)
import WASILibc
#else
import Foundation
import CoreGraphics
#endif

public enum BlendMode: String, Sendable {
    case normal, multiply, screen, overlay, darken, lighten, colorDodge, colorBurn, softLight, hardLight
    case difference, exclusion, hue, saturation, color, luminosity
    case sourceAtop, destinationOver, destinationOut, plusDarker, plusLighter
}

public enum ColorRenderingMode: String, Equatable, Sendable {
    case nonLinear, linear, extendedLinear
}

enum _PaintEffect: Sendable {
    enum Amount: String, Sendable {
        case brightness, contrast, saturation, grayscale, opacity
    }

    case affine(CGAffineTransform, anchor: UnitPoint)
    case projection(ProjectionTransform)
    case blur(CGFloat, opaque: Bool)
    case blend(String)
    case group(opaque: Bool = false, colorMode: ColorRenderingMode = .nonLinear)
    case hueRotation(Double)
    case value(Amount, Double)
    case colorInvert
    case luminanceToAlpha

    static func rotation(_ angle: Angle, anchor: UnitPoint) -> Self {
        .affine(CGAffineTransform(rotationAngle: CGFloat(angle.radians)), anchor: anchor)
    }

    static func scale(_ x: CGFloat, _ y: CGFloat, anchor: UnitPoint) -> Self {
        .affine(CGAffineTransform(scaleX: x, y: y), anchor: anchor)
    }

    var json: Node {
        func finite(_ number: Double) -> Double { number.isFinite ? number : 0 }
        switch self {
        case let .affine(transform, anchor):
            let matrix = [transform.a, transform.b, transform.c, transform.d, transform.tx, transform.ty]
            return ["kind": "affine", "matrix": matrix.map { finite(Double($0)) },
                    "anchor": anchor.paintPoint.map(finite)]
        case .projection(let transform):
            return ["kind": "projection", "matrix": transform.coefficients.map(finite)]
        case let .blur(radius, opaque):
            return ["kind": "blur", "radius": finite(Double(radius)), "opaque": opaque]
        case .blend(let mode):
            return ["kind": "blend", "mode": mode]
        case let .group(opaque, colorMode):
            return ["kind": "group", "opaque": opaque, "colorMode": colorMode.rawValue]
        case .hueRotation(let radians):
            return ["kind": "hueRotation", "radians": finite(radians)]
        case let .value(kind, amount):
            return ["kind": kind.rawValue, "amount": finite(amount)]
        case .colorInvert: return ["kind": "colorInvert"]
        case .luminanceToAlpha: return ["kind": "luminanceToAlpha"]
        }
    }
}

public struct ProjectionTransform: Equatable, Sendable {
    public var m11: CGFloat = 1, m12: CGFloat = 0, m13: CGFloat = 0
    public var m21: CGFloat = 0, m22: CGFloat = 1, m23: CGFloat = 0
    public var m31: CGFloat = 0, m32: CGFloat = 0, m33: CGFloat = 1

    public init() {}

    public init(_ transform: CGAffineTransform) {
        m11 = transform.a; m12 = transform.b
        m21 = transform.c; m22 = transform.d
        m31 = transform.tx; m32 = transform.ty
    }

    var coefficients: [Double] {
        [m11, m12, m13, m21, m22, m23, m31, m32, m33].map { Double($0) }
    }

    public var isIdentity: Bool { self == ProjectionTransform() }
    public var isAffine: Bool { m13 == 0 && m23 == 0 && m33 == 1 }

    public func concatenating(_ other: Self) -> Self {
        let a = coefficients, b = other.coefficients
        var result = [CGFloat]()
        for row in 0..<3 {
            for column in 0..<3 {
                let index = row * 3
                result.append(CGFloat(a[index] * b[column] + a[index + 1] * b[column + 3] + a[index + 2] * b[column + 6]))
            }
        }
        return Self(result)
    }

    public mutating func invert() -> Bool {
        let adjugate = [
            m22 * m33 - m23 * m32, m13 * m32 - m12 * m33, m12 * m23 - m13 * m22,
            m23 * m31 - m21 * m33, m11 * m33 - m13 * m31, m13 * m21 - m11 * m23,
            m21 * m32 - m22 * m31, m12 * m31 - m11 * m32, m11 * m22 - m12 * m21,
        ]
        let determinant = m11 * adjugate[0] + m12 * adjugate[3] + m13 * adjugate[6]
        guard determinant != 0 && determinant.isFinite else { return false }
        self = Self(adjugate.map { $0 / determinant })
        return true
    }

    public func inverted() -> Self {
        var result = self
        _ = result.invert()
        return result
    }

    private init(_ m: [CGFloat]) {
        m11 = m[0]; m12 = m[1]; m13 = m[2]
        m21 = m[3]; m22 = m[4]; m23 = m[5]
        m31 = m[6]; m32 = m[7]; m33 = m[8]
    }
}

extension View {
    func _effect(_ effect: _PaintEffect) -> some View { _modified { ["effect": effect.json] } }

    public func transformEffect(_ transform: CGAffineTransform) -> some View { _effect(.affine(transform, anchor: .zero)) }
    public func projectionEffect(_ transform: ProjectionTransform) -> some View {
        _effect(.projection(transform))
    }
    public func rotationEffect(_ angle: Angle, anchor: UnitPoint = .center) -> some View {
        _effect(.rotation(angle, anchor: anchor))
    }
    public func scaleEffect(_ scale: CGSize, anchor: UnitPoint = .center) -> some View {
        scaleEffect(x: scale.width, y: scale.height, anchor: anchor)
    }
    public func scaleEffect(_ scale: CGFloat, anchor: UnitPoint = .center) -> some View {
        scaleEffect(x: scale, y: scale, anchor: anchor)
    }
    public func scaleEffect(x: CGFloat = 1, y: CGFloat = 1, anchor: UnitPoint = .center) -> some View {
        _effect(.scale(x, y, anchor: anchor))
    }
    public func blur(radius: CGFloat, opaque: Bool = false) -> some View { _effect(.blur(radius, opaque: opaque)) }
    public func blendMode(_ mode: BlendMode) -> some View { _effect(.blend(mode.rawValue)) }
    public func brightness(_ amount: Double) -> some View { _effect(.value(.brightness, amount)) }
    public func contrast(_ amount: Double) -> some View { _effect(.value(.contrast, amount)) }
    public func saturation(_ amount: Double) -> some View { _effect(.value(.saturation, amount)) }
    public func grayscale(_ amount: Double) -> some View { _effect(.value(.grayscale, amount)) }
    public func hueRotation(_ angle: Angle) -> some View { _effect(.hueRotation(angle.radians)) }
    public func colorMultiply(_ color: Color) -> some View {
        _modified { ["effect": ["kind": "colorMultiply", "color": color.json]] }
    }
    public func colorInvert() -> some View { _effect(.colorInvert) }
    public func luminanceToAlpha() -> some View { _effect(.luminanceToAlpha) }
    public func compositingGroup() -> some View { _effect(.group()) }
    public func drawingGroup(opaque: Bool = false, colorMode: ColorRenderingMode = .nonLinear) -> some View {
        _effect(.group(opaque: opaque, colorMode: colorMode))
    }

    public func mask<M: View>(alignment: Alignment = .center, @ViewBuilder _ mask: () -> M) -> some View {
        let view = mask()
        return _modifiedAt { ["mask": ["view": _eval(view, $0 + "/mask"), "alignment": alignment.name]] }
    }
    public func mask<M: View>(_ mask: M) -> some View { self.mask { mask } }

    public func clipShape<S: Shape>(_ shape: S, style: FillStyle = FillStyle()) -> some View {
        mask { shape.fill(Color.white, style: style) }
    }
    public func clipped(antialiased: Bool = false) -> some View {
        clipShape(Rectangle(), style: FillStyle(antialiased: antialiased))
    }

    public func visualEffect(_ effect: @escaping (EmptyVisualEffect, GeometryProxy) -> some VisualEffect) -> some View {
        _VisualEffectView(content: self, effect: effect)
    }
}

public protocol VisualEffect: Sendable, Animatable {
    var _effects: [[String: Any]] { get }
}

public struct EmptyVisualEffect: VisualEffect {
    public init() {}
    public var _effects: [[String: Any]] { [] }
}

private struct _CombinedVisualEffect<Base: VisualEffect>: VisualEffect {
    let base: Base
    let effect: _PaintEffect
    var _effects: [[String: Any]] { base._effects + [effect.json] }
}

extension VisualEffect {
    private func appending(_ effect: _PaintEffect) -> some VisualEffect { _CombinedVisualEffect(base: self, effect: effect) }

    public func offset(_ offset: CGSize) -> some VisualEffect { self.offset(x: offset.width, y: offset.height) }
    public func offset(x: CGFloat = 0, y: CGFloat = 0) -> some VisualEffect {
        transformEffect(CGAffineTransform(translationX: x, y: y))
    }
    public func transformEffect(_ transform: CGAffineTransform) -> some VisualEffect {
        appending(.affine(transform, anchor: .zero))
    }
    public func projectionEffect(_ transform: ProjectionTransform) -> some VisualEffect {
        appending(.projection(transform))
    }
    public func rotationEffect(_ angle: Angle, anchor: UnitPoint = .center) -> some VisualEffect {
        appending(.rotation(angle, anchor: anchor))
    }
    public func scaleEffect(_ scale: CGSize, anchor: UnitPoint = .center) -> some VisualEffect {
        scaleEffect(x: scale.width, y: scale.height, anchor: anchor)
    }
    public func scaleEffect(_ scale: CGFloat, anchor: UnitPoint = .center) -> some VisualEffect {
        scaleEffect(x: scale, y: scale, anchor: anchor)
    }
    public func scaleEffect(x: CGFloat = 1, y: CGFloat = 1, anchor: UnitPoint = .center) -> some VisualEffect {
        appending(.scale(x, y, anchor: anchor))
    }
    public func blur(radius: CGFloat, opaque: Bool = false) -> some VisualEffect { appending(.blur(radius, opaque: opaque)) }
    public func blendMode(_ mode: BlendMode) -> some VisualEffect { appending(.blend(mode.rawValue)) }
    public func opacity(_ amount: Double) -> some VisualEffect { appending(.value(.opacity, amount)) }
    public func brightness(_ amount: Double) -> some VisualEffect { appending(.value(.brightness, amount)) }
    public func contrast(_ amount: Double) -> some VisualEffect { appending(.value(.contrast, amount)) }
    public func saturation(_ amount: Double) -> some VisualEffect { appending(.value(.saturation, amount)) }
    public func grayscale(_ amount: Double) -> some VisualEffect { appending(.value(.grayscale, amount)) }
    public func hueRotation(_ angle: Angle) -> some VisualEffect { appending(.hueRotation(angle.radians)) }
    public func colorInvert() -> some VisualEffect { appending(.colorInvert) }
    public func luminanceToAlpha() -> some VisualEffect { appending(.luminanceToAlpha) }
}

private struct _VisualEffectView<Content: View, Effect: VisualEffect>: View, _Primitive {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content
    let effect: (EmptyVisualEffect, GeometryProxy) -> Effect

    func _emit(_ path: String) -> Node {
        let runtime = _Runtime.shared
        runtime.visualEffectIDs.insert(path)
        var geometry = runtime.fallbackGeo()
        if let frame = runtime.visualEffectFrames[path] {
            geometry.x = Double(frame.minX); geometry.y = Double(frame.minY)
            geometry.w = Double(frame.width); geometry.h = Double(frame.height)
        }
        return ["kind": "visualEffect", "id": path, "child": _eval(content, path + "/content"),
                "frame": [geometry.x, geometry.y, geometry.w, geometry.h],
                "effects": effect(.init(), GeometryProxy(geo: geometry))._effects]
    }
}
