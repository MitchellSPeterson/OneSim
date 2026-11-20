// WASI build (docs/web.md): no Foundation (it is huge). Minimal stand-ins for what the module used from it.
#if os(WASI)
import WASILibc
public typealias CGFloat = Double

public struct CGPoint: Equatable, Hashable, Sendable {
    public var x: CGFloat, y: CGFloat
    public init() { x = 0; y = 0 }
    public init(x: CGFloat, y: CGFloat) { self.x = x; self.y = y }
    public static let zero = CGPoint()
}

public struct CGSize: Equatable, Hashable, Sendable {
    public var width: CGFloat, height: CGFloat
    public init() { width = 0; height = 0 }
    public init(width: CGFloat, height: CGFloat) { self.width = width; self.height = height }
    public static let zero = CGSize()
}

public struct CGRect: Equatable, Hashable, Sendable {
    public var origin: CGPoint
    public var size: CGSize
    public init() { origin = .zero; size = .zero }
    public init(origin: CGPoint, size: CGSize) { self.origin = origin; self.size = size }
    public init(x: CGFloat, y: CGFloat, width: CGFloat, height: CGFloat) {
        self.origin = CGPoint(x: x, y: y); self.size = CGSize(width: width, height: height)
    }
    public static let zero = CGRect()
    public static let null = CGRect(x: .infinity, y: .infinity, width: 0, height: 0)
    public var isNull: Bool { origin.x.isInfinite || origin.y.isInfinite }
    public var isEmpty: Bool { isNull || size.width == 0 || size.height == 0 }
    public var standardized: CGRect {
        guard !isNull else { return .null }
        let x = size.width < 0 ? origin.x + size.width : origin.x
        let y = size.height < 0 ? origin.y + size.height : origin.y
        return CGRect(x: x, y: y, width: abs(size.width), height: abs(size.height))
    }
    public var width: CGFloat { standardized.size.width }
    public var height: CGFloat { standardized.size.height }
    public var minX: CGFloat { standardized.origin.x }
    public var midX: CGFloat { let s = standardized; return s.origin.x + s.size.width / 2 }
    public var maxX: CGFloat { let s = standardized; return s.origin.x + s.size.width }
    public var minY: CGFloat { standardized.origin.y }
    public var midY: CGFloat { let s = standardized; return s.origin.y + s.size.height / 2 }
    public var maxY: CGFloat { let s = standardized; return s.origin.y + s.size.height }
    public func insetBy(dx: CGFloat, dy: CGFloat) -> CGRect {
        guard !isNull else { return .null }
        let s = standardized
        let nw = s.size.width - 2 * dx, nh = s.size.height - 2 * dy
        if nw < 0 || nh < 0 { return .null }
        return CGRect(x: s.origin.x + dx, y: s.origin.y + dy, width: nw, height: nh)
    }
    public func offsetBy(dx: CGFloat, dy: CGFloat) -> CGRect {
        guard !isNull else { return .null }
        return CGRect(x: origin.x + dx, y: origin.y + dy, width: size.width, height: size.height)
    }
    public func contains(_ p: CGPoint) -> Bool {
        !isEmpty && p.x >= minX && p.x < maxX && p.y >= minY && p.y < maxY
    }
    public func union(_ r: CGRect) -> CGRect {
        if isNull { return r }
        if r.isNull { return self }
        let x0 = min(minX, r.minX), y0 = min(minY, r.minY)
        let x1 = max(maxX, r.maxX), y1 = max(maxY, r.maxY)
        return CGRect(x: x0, y: y0, width: x1 - x0, height: y1 - y0)
    }
}

/// Seconds since 1970 (gesture value times).
public struct Date: Equatable, Hashable, Comparable {
    public var timeIntervalSince1970: Double
    public init(timeIntervalSince1970 t: Double) { timeIntervalSince1970 = t }
    public func timeIntervalSince(_ d: Date) -> Double { timeIntervalSince1970 - d.timeIntervalSince1970 }
    public static func < (a: Date, b: Date) -> Bool { a.timeIntervalSince1970 < b.timeIntervalSince1970 }
}
struct NSNull {}  // serialized as null by _json

/// Compact JSON for the `Node` tree (the only shapes the module emits: strings, numbers, bools, arrays, objects).
func _json(_ v: Any, _ out: inout String) {
    switch v {
    case let s as String: _jsonString(s, &out)
    case let b as Bool: out += b ? "true" : "false"
    case let i as Int: out += String(i)
    case let u as UInt64: out += String(u)
    case let d as Double: out += d.isFinite ? String(d) : "null"
    case let n as [String: Any]:
        out += "{"
        var first = true
        for (k, x) in n {
            if !first { out += "," }
            first = false
            _jsonString(k, &out); out += ":"; _json(x, &out)
        }
        out += "}"
    case let a as [Any]:
        out += "["
        for (i, x) in a.enumerated() { if i > 0 { out += "," }; _json(x, &out) }
        out += "]"
    default: out += "null"
    }
}

private func _jsonString(_ s: String, _ out: inout String) {
    out += "\""
    for u in s.unicodeScalars {
        switch u {
        case "\"": out += "\\\""
        case "\\": out += "\\\\"
        case "\n": out += "\\n"
        case "\r": out += "\\r"
        case "\t": out += "\\t"
        default:
            if u.value < 0x20 {
                let h = String(u.value, radix: 16)
                out += "\\u" + String(repeating: "0", count: 4 - h.count) + h
            } else { out.unicodeScalars.append(u) }
        }
    }
    out += "\""
}
#endif

#if !canImport(CoreGraphics)
#if !os(WASI)
import Foundation
#endif

public enum CGLineCap: Int32, Equatable, Hashable, Sendable {
    case butt = 0
    case round = 1
    case square = 2
}

public enum CGLineJoin: Int32, Equatable, Hashable, Sendable {
    case miter = 0
    case round = 1
    case bevel = 2
}

public struct CGAffineTransform: Equatable, Hashable, Sendable {
    public var a: CGFloat, b: CGFloat, c: CGFloat, d: CGFloat, tx: CGFloat, ty: CGFloat
    public init() { a = 1; b = 0; c = 0; d = 1; tx = 0; ty = 0 }
    public init(a: CGFloat, b: CGFloat, c: CGFloat, d: CGFloat, tx: CGFloat, ty: CGFloat) {
        self.a = a; self.b = b; self.c = c; self.d = d; self.tx = tx; self.ty = ty
    }
    public init(translationX tx: CGFloat, y ty: CGFloat) { self.init(a: 1, b: 0, c: 0, d: 1, tx: tx, ty: ty) }
    public init(scaleX sx: CGFloat, y sy: CGFloat) { self.init(a: sx, b: 0, c: 0, d: sy, tx: 0, ty: 0) }
    public init(rotationAngle angle: CGFloat) {
        let cs = CGFloat(cos(Double(angle))), sn = CGFloat(sin(Double(angle)))
        self.init(a: cs, b: sn, c: -sn, d: cs, tx: 0, ty: 0)
    }
    public static let identity = CGAffineTransform()
    public var isIdentity: Bool { self == .identity }
    public func concatenating(_ t2: CGAffineTransform) -> CGAffineTransform {
        CGAffineTransform(
            a: a * t2.a + b * t2.c, b: a * t2.b + b * t2.d,
            c: c * t2.a + d * t2.c, d: c * t2.b + d * t2.d,
            tx: tx * t2.a + ty * t2.c + t2.tx, ty: tx * t2.b + ty * t2.d + t2.ty
        )
    }
    public func translatedBy(x: CGFloat, y: CGFloat) -> CGAffineTransform {
        CGAffineTransform(translationX: x, y: y).concatenating(self)
    }
    public func scaledBy(x: CGFloat, y: CGFloat) -> CGAffineTransform {
        CGAffineTransform(scaleX: x, y: y).concatenating(self)
    }
    public func rotated(by angle: CGFloat) -> CGAffineTransform {
        CGAffineTransform(rotationAngle: angle).concatenating(self)
    }
    public func inverted() -> CGAffineTransform {
        let det = a * d - b * c
        guard abs(det) > 1e-12 else { return self }
        return CGAffineTransform(
            a: d / det, b: -b / det, c: -c / det, d: a / det,
            tx: (c * ty - d * tx) / det, ty: (b * tx - a * ty) / det
        )
    }
}

extension CGPoint {
    public func applying(_ t: CGAffineTransform) -> CGPoint {
        CGPoint(x: t.a * x + t.c * y + t.tx, y: t.b * x + t.d * y + t.ty)
    }
}

extension CGRect {
    public func applying(_ t: CGAffineTransform) -> CGRect {
        guard !isNull else { return .null }
        let p0 = CGPoint(x: minX, y: minY).applying(t)
        let p1 = CGPoint(x: maxX, y: minY).applying(t)
        let p2 = CGPoint(x: maxX, y: maxY).applying(t)
        let p3 = CGPoint(x: minX, y: maxY).applying(t)
        let x0 = min(min(p0.x, p1.x), min(p2.x, p3.x)), y0 = min(min(p0.y, p1.y), min(p2.y, p3.y))
        let x1 = max(max(p0.x, p1.x), max(p2.x, p3.x)), y1 = max(max(p0.y, p1.y), max(p2.y, p3.y))
        return CGRect(x: x0, y: y0, width: x1 - x0, height: y1 - y0)
    }
}
#endif
