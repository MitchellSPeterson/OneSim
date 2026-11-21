// Apple's SwiftUI re-exports Foundation; apps use `URL` (Link) without importing it.
#if !os(WASI)
@_exported import struct Foundation.URL
#endif

// WASI build (docs/web.md): no Foundation (it is huge). Minimal stand-ins for what the module used from it.
#if os(WASI)
import WASILibc
public typealias CGFloat = Double
public typealias TimeInterval = Double

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
    public init(origin: CGPoint = .zero, size: CGSize = .zero) { self.origin = origin; self.size = size }
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
    public var width: CGFloat {
        get { standardized.size.width }
        set { size.width = newValue }
    }
    public var height: CGFloat {
        get { standardized.size.height }
        set { size.height = newValue }
    }
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

/// Seconds since 1970 (gesture value times and TimelineView dates).
public struct Date: Equatable, Hashable, Comparable, Sendable {
    public var timeIntervalSince1970: Double
    public init() { timeIntervalSince1970 = _Runtime.shared.baseEpoch + _Runtime.shared.clockMs / 1000.0 }
    public init(timeIntervalSince1970 t: Double) { timeIntervalSince1970 = t }
    public static var now: Date { Date() }
    public static let distantPast = Date(timeIntervalSince1970: -62135769600.0)
    public static let distantFuture = Date(timeIntervalSince1970: 64092211200.0)
    public func timeIntervalSince(_ d: Date) -> Double { timeIntervalSince1970 - d.timeIntervalSince1970 }
    public func addingTimeInterval(_ ti: TimeInterval) -> Date { Date(timeIntervalSince1970: timeIntervalSince1970 + ti) }
    public static func + (d: Date, ti: TimeInterval) -> Date { d.addingTimeInterval(ti) }
    public static func - (a: Date, b: Date) -> TimeInterval { a.timeIntervalSince(b) }
    public static func < (a: Date, b: Date) -> Bool { a.timeIntervalSince1970 < b.timeIntervalSince1970 }
}
struct NSNull {}  // serialized as null by _json

/// A fixed offset from GMT (WASI has no time zone database). `.current` is GMT.
public struct TimeZone: Hashable, Sendable {
    let seconds: Int
    public init?(secondsFromGMT seconds: Int) {
        guard (-18 * 3600...18 * 3600).contains(seconds) else { return nil }
        self.seconds = seconds
    }
    /// Only "GMT", "UTC" and "GMT+hh:mm" / "GMT-hhmm" style identifiers.
    public init?(identifier: String) {
        guard let s = _gmtOffset(identifier) else { return nil }
        seconds = s
    }
    public init?(abbreviation: String) { self.init(identifier: abbreviation) }
    public static var current: TimeZone { TimeZone(secondsFromGMT: 0)! }
    public static var gmt: TimeZone { TimeZone(secondsFromGMT: 0)! }
    public var identifier: String { seconds == 0 ? "GMT" : "GMT\(seconds < 0 ? "-" : "+")\(_pad2(abs(seconds) / 3600))\(_pad2(abs(seconds) % 3600 / 60))" }
    public func secondsFromGMT(for date: Date = Date()) -> Int { seconds }
    public func secondsFromGMT() -> Int { seconds }
}
func _gmtOffset(_ id: String) -> Int? {
    if id == "GMT" || id == "UTC" || id == "Z" { return 0 }
    guard id.hasPrefix("GMT") || id.hasPrefix("UTC") else { return nil }
    let rest = Array(id.dropFirst(3))
    guard let sign = rest.first, sign == "+" || sign == "-" else { return nil }
    let digits = rest.dropFirst().filter { $0 != ":" }
    guard !digits.isEmpty, digits.count <= 4, digits.allSatisfy({ $0.isASCII && $0.isNumber }) else { return nil }
    let v = Int(String(digits))!
    let (h, m) = digits.count <= 2 ? (v, 0) : (v / 100, v % 100)
    guard h <= 18, m < 60, h < 18 || m == 0 else { return nil }
    return (sign == "-" ? -1 : 1) * (h * 3600 + m * 60)
}

/// A language-region identifier; WASI formats every locale as `en_US`.
public struct Locale: Hashable, Sendable {
    public let identifier: String
    public init(identifier: String) { self.identifier = identifier }
    public static var current: Locale { Locale(identifier: "en_US") }
    public static var autoupdatingCurrent: Locale { current }
}

/// The Gregorian calendar only (WASI has no ICU).
public struct Calendar: Hashable, Sendable {
    public enum Identifier: Hashable, Sendable { case gregorian, iso8601 }
    public let identifier: Identifier
    public var timeZone = TimeZone.current
    public var locale: Locale? = Locale.current
    /// 1 = Sunday ... 7 = Saturday.
    public var firstWeekday = 1
    public init(identifier: Identifier) {
        self.identifier = identifier
        if identifier == .iso8601 { firstWeekday = 2 }
    }
    public static var current: Calendar { Calendar(identifier: .gregorian) }
    public static var autoupdatingCurrent: Calendar { current }
}

/// Calendar fields (`MultiDatePicker` selections). Unset fields are nil.
public struct DateComponents: Hashable, Sendable {
    public var calendar: Calendar?, timeZone: TimeZone?
    public var era: Int?, year: Int?, month: Int?, day: Int?, hour: Int?, minute: Int?, second: Int?, nanosecond: Int?
    public var weekday: Int?, weekdayOrdinal: Int?, quarter: Int?, weekOfMonth: Int?, weekOfYear: Int?, yearForWeekOfYear: Int?
    public init(calendar: Calendar? = nil, timeZone: TimeZone? = nil, era: Int? = nil, year: Int? = nil, month: Int? = nil,
                day: Int? = nil, hour: Int? = nil, minute: Int? = nil, second: Int? = nil, nanosecond: Int? = nil,
                weekday: Int? = nil, weekdayOrdinal: Int? = nil, quarter: Int? = nil, weekOfMonth: Int? = nil,
                weekOfYear: Int? = nil, yearForWeekOfYear: Int? = nil) {
        self.calendar = calendar; self.timeZone = timeZone; self.era = era; self.year = year; self.month = month
        self.day = day; self.hour = hour; self.minute = minute; self.second = second; self.nanosecond = nanosecond
        self.weekday = weekday; self.weekdayOrdinal = weekdayOrdinal; self.quarter = quarter; self.weekOfMonth = weekOfMonth
        self.weekOfYear = weekOfYear; self.yearForWeekOfYear = yearForWeekOfYear
    }
}

/// Foundation's `IndexSet` (onDelete / onMove offsets): a sorted set of non-negative positions.
public struct IndexSet: Equatable, Hashable, Sequence, ExpressibleByArrayLiteral, Sendable {
    private var items: [Int] = []
    public init() {}
    public init(integer: Int) { items = [integer] }
    public init<S: Sequence>(_ s: S) where S.Element == Int { for i in s { insert(i) } }
    public init(arrayLiteral elements: Int...) { self.init(elements) }
    public var count: Int { items.count }
    public var isEmpty: Bool { items.isEmpty }
    public var first: Int? { items.first }
    public var last: Int? { items.last }
    public func contains(_ i: Int) -> Bool { items.contains(i) }
    @discardableResult public mutating func insert(_ i: Int) -> (inserted: Bool, memberAfterInsert: Int) {
        if items.contains(i) { return (false, i) }
        items.insert(i, at: items.firstIndex { $0 > i } ?? items.count)
        return (true, i)
    }
    public mutating func remove(_ i: Int) { items.removeAll { $0 == i } }
    public func makeIterator() -> IndexingIterator<[Int]> { items.makeIterator() }
}

/// `Link` destinations. Only the string is kept.
public struct URL: Equatable, Hashable {
    public let absoluteString: String
    public init?(string: String) { if string.isEmpty { return nil }; absoluteString = string }
}

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
    case is NSNull: out += "null"
    default: out += "null"
    }
}

private let _hexDigits: [UnicodeScalar] = Array("0123456789abcdef".unicodeScalars)

private func _jsonString(_ s: String, _ out: inout String) {
    out.reserveCapacity(out.utf8.count + s.utf8.count + 2)
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
                out += "\\u00"
                out.unicodeScalars.append(_hexDigits[Int((u.value >> 4) & 0xf)])
                out.unicodeScalars.append(_hexDigits[Int(u.value & 0xf)])
            } else { out.unicodeScalars.append(u) }
        }
    }
    out += "\""
}

/// Host layout lines (`geometry`, `scroll_position`). Enough JSON for objects, arrays, strings, numbers, bools, null.
func _jsonParse(_ s: String) -> Any? {
    var i = s.startIndex
    func peek() -> Character? { i < s.endIndex ? s[i] : nil }
    func bump() -> Character? {
        guard i < s.endIndex else { return nil }
        let c = s[i]; i = s.index(after: i); return c
    }
    func skip() { while let c = peek(), c == " " || c == "\n" || c == "\r" || c == "\t" { _ = bump() } }
    func parseString() -> String? {
        guard bump() == "\"" else { return nil }
        var out = ""
        while let c = bump() {
            if c == "\"" { return out }
            if c != "\\" { out.append(c); continue }
            guard let e = bump() else { return nil }
            switch e {
            case "\"": out.append("\"")
            case "\\": out.append("\\")
            case "/": out.append("/")
            case "n": out.append("\n")
            case "r": out.append("\r")
            case "t": out.append("\t")
            case "u":
                var hex = ""
                for _ in 0..<4 { guard let h = bump() else { return nil }; hex.append(h) }
                if let v = UInt32(hex, radix: 16), let u = UnicodeScalar(v) { out.unicodeScalars.append(u) }
            default: out.append(e)
            }
        }
        return nil
    }
    func parseNumber() -> Double? {
        let start = i
        if peek() == "-" { _ = bump() }
        while let c = peek(), c.isNumber || c == "." || c == "e" || c == "E" || c == "+" || c == "-" { _ = bump() }
        guard i > start else { return nil }
        return Double(s[start..<i])
    }
    func parseValue() -> Any? {
        skip()
        guard let c = peek() else { return nil }
        if c == "\"" { return parseString() }
        if c == "{" { return parseObject() }
        if c == "[" { return parseArray() }
        if c == "t" { for _ in 0..<4 { _ = bump() }; return true }
        if c == "f" { for _ in 0..<5 { _ = bump() }; return false }
        if c == "n" { for _ in 0..<4 { _ = bump() }; return NSNull() }
        return parseNumber()
    }
    func parseObject() -> [String: Any]? {
        _ = bump()
        var obj: [String: Any] = [:]
        skip()
        if peek() == "}" { _ = bump(); return obj }
        while true {
            skip()
            guard let key = parseString() else { return nil }
            skip()
            guard bump() == ":" else { return nil }
            guard let v = parseValue() else { return nil }
            obj[key] = v
            skip()
            switch bump() {
            case "}": return obj
            case ",": continue
            default: return nil
            }
        }
    }
    func parseArray() -> [Any]? {
        _ = bump()
        var arr: [Any] = []
        skip()
        if peek() == "]" { _ = bump(); return arr }
        while true {
            guard let v = parseValue() else { return nil }
            arr.append(v)
            skip()
            switch bump() {
            case "]": return arr
            case ",": continue
            default: return nil
            }
        }
    }
    return parseValue()
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
