// WASI build (docs/web.md): no Foundation (it is huge). Minimal stand-ins for what the module used from it.
#if os(WASI)
public typealias CGFloat = Double
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
