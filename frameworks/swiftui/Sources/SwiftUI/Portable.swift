// Apple's SwiftUI re-exports Foundation; apps use `URL` (Link) without importing it.
#if !os(WASI)
@_exported import struct Foundation.URL
#endif

// WASI build (docs/web.md): no Foundation (it is huge). Minimal stand-ins for what the module used from it.
#if os(WASI)
public typealias CGFloat = Double

public struct CGPoint: Equatable, Hashable {
    public var x: CGFloat, y: CGFloat
    public init() { x = 0; y = 0 }
    public init(x: CGFloat, y: CGFloat) { self.x = x; self.y = y }
    public static let zero = CGPoint()
}

public struct CGSize: Equatable, Hashable {
    public var width: CGFloat, height: CGFloat
    public init() { width = 0; height = 0 }
    public init(width: CGFloat, height: CGFloat) { self.width = width; self.height = height }
    public static let zero = CGSize()
}

/// Seconds since 1970 (gesture value times).
public struct Date: Equatable, Hashable, Comparable {
    public var timeIntervalSince1970: Double
    public init(timeIntervalSince1970 t: Double) { timeIntervalSince1970 = t }
    public func timeIntervalSince(_ d: Date) -> Double { timeIntervalSince1970 - d.timeIntervalSince1970 }
    public static func < (a: Date, b: Date) -> Bool { a.timeIntervalSince1970 < b.timeIntervalSince1970 }
}
struct NSNull {}  // serialized as null by _json

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
