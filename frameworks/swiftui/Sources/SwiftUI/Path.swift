#if os(WASI)
import WASILibc
#else
import Foundation
#endif
#if canImport(CoreGraphics)
import CoreGraphics
#endif

// MARK: - Path

public struct Path: Equatable, LosslessStringConvertible, Sendable {
    public enum Element: Equatable, Sendable {
        case move(to: CGPoint)
        case line(to: CGPoint)
        case quadCurve(to: CGPoint, control: CGPoint)
        case curve(to: CGPoint, control1: CGPoint, control2: CGPoint)
        case closeSubpath
    }

    var elements: [Element] = []

    public init() {}

    public init(_ callback: (inout Path) -> ()) {
        var p = Path()
        callback(&p)
        self = p
    }

    public init(_ rect: CGRect) {
        self.init()
        addRect(rect)
    }

    public init(roundedRect rect: CGRect, cornerSize: CGSize, style: RoundedCornerStyle = .continuous) {
        self.init()
        addRoundedRect(in: rect, cornerSize: cornerSize, style: style)
    }

    public init(roundedRect rect: CGRect, cornerRadius: CGFloat, style: RoundedCornerStyle = .continuous) {
        self.init(roundedRect: rect, cornerSize: CGSize(width: cornerRadius, height: cornerRadius), style: style)
    }

    public init(roundedRect rect: CGRect, cornerRadii: RectangleCornerRadii, style: RoundedCornerStyle = .continuous) {
        self.init()
        addRoundedRect(in: rect, cornerRadii: cornerRadii, style: style)
    }

    public init(ellipseIn rect: CGRect) {
        self.init()
        addEllipse(in: rect)
    }

    /// Parses SwiftUI's `Path.description` postfix format (`x y m`, `x y l`, `cx cy x y q`, `c1x c1y c2x c2y x y c`, `h`).
    public init?(_ string: String) {
        var p = Path()
        var nums: [CGFloat] = []
        for tok in string.split(whereSeparator: { $0.isWhitespace }) {
            switch tok {
            case "m":
                guard nums.count == 2 else { return nil }
                p.move(to: CGPoint(x: nums[0], y: nums[1]))
                nums.removeAll(keepingCapacity: true)
            case "l":
                guard nums.count == 2 else { return nil }
                p.addLine(to: CGPoint(x: nums[0], y: nums[1]))
                nums.removeAll(keepingCapacity: true)
            case "q":
                guard nums.count == 4 else { return nil }
                p.addQuadCurve(to: CGPoint(x: nums[2], y: nums[3]), control: CGPoint(x: nums[0], y: nums[1]))
                nums.removeAll(keepingCapacity: true)
            case "c":
                guard nums.count == 6 else { return nil }
                p.addCurve(
                    to: CGPoint(x: nums[4], y: nums[5]),
                    control1: CGPoint(x: nums[0], y: nums[1]),
                    control2: CGPoint(x: nums[2], y: nums[3])
                )
                nums.removeAll(keepingCapacity: true)
            case "h":
                guard nums.isEmpty else { return nil }
                p.closeSubpath()
            default:
                guard let v = Double(tok) else { return nil }
                nums.append(CGFloat(v))
            }
        }
        guard nums.isEmpty else { return nil }
        self = p
    }

    public var isEmpty: Bool { elements.isEmpty }

    public var currentPoint: CGPoint? {
        var cur: CGPoint?
        var subStart: CGPoint?
        for e in elements {
            switch e {
            case .move(let p): cur = p; subStart = p
            case .line(let p): cur = p
            case .quadCurve(let p, _): cur = p
            case .curve(let p, _, _): cur = p
            case .closeSubpath: cur = subStart
            }
        }
        return cur
    }

    public func forEach(_ body: (Path.Element) -> Void) {
        for e in elements { body(e) }
    }

    public var description: String {
        func fmt(_ v: CGFloat) -> String {
            let d = Double(v)
            if d == 0 { return "0" }
            if d.rounded() == d && abs(d) < 1e12 { return String(Int64(d)) }
            return String(d)
        }
        var parts: [String] = []
        parts.reserveCapacity(elements.count * 4)
        for e in elements {
            switch e {
            case .move(let p):
                parts.append(fmt(p.x)); parts.append(fmt(p.y)); parts.append("m")
            case .line(let p):
                parts.append(fmt(p.x)); parts.append(fmt(p.y)); parts.append("l")
            case .quadCurve(let p, let c):
                parts.append(fmt(c.x)); parts.append(fmt(c.y))
                parts.append(fmt(p.x)); parts.append(fmt(p.y)); parts.append("q")
            case .curve(let p, let c1, let c2):
                parts.append(fmt(c1.x)); parts.append(fmt(c1.y))
                parts.append(fmt(c2.x)); parts.append(fmt(c2.y))
                parts.append(fmt(p.x)); parts.append(fmt(p.y)); parts.append("c")
            case .closeSubpath:
                parts.append("h")
            }
        }
        return parts.joined(separator: " ")
    }

    public var boundingRect: CGRect {
        guard !elements.isEmpty else { return .null }
        var minX = CGFloat.infinity, minY = CGFloat.infinity
        var maxX = -CGFloat.infinity, maxY = -CGFloat.infinity
        func include(_ x: CGFloat, _ y: CGFloat) {
            if x < minX { minX = x }; if x > maxX { maxX = x }
            if y < minY { minY = y }; if y > maxY { maxY = y }
        }
        var cur = CGPoint.zero
        var subStart = CGPoint.zero
        for e in elements {
            switch e {
            case .move(let p):
                cur = p; subStart = p; include(p.x, p.y)
            case .line(let p):
                include(cur.x, cur.y); include(p.x, p.y); cur = p
            case .quadCurve(let p, let c):
                include(cur.x, cur.y); include(p.x, p.y)
                for t in _quadExtrema(cur.x, c.x, p.x) {
                    include(_evalQuad(cur.x, c.x, p.x, t), _evalQuad(cur.y, c.y, p.y, t))
                }
                for t in _quadExtrema(cur.y, c.y, p.y) {
                    include(_evalQuad(cur.x, c.x, p.x, t), _evalQuad(cur.y, c.y, p.y, t))
                }
                cur = p
            case .curve(let p, let c1, let c2):
                include(cur.x, cur.y); include(p.x, p.y)
                for t in _cubicExtrema(cur.x, c1.x, c2.x, p.x) {
                    include(_evalCubic(cur.x, c1.x, c2.x, p.x, t), _evalCubic(cur.y, c1.y, c2.y, p.y, t))
                }
                for t in _cubicExtrema(cur.y, c1.y, c2.y, p.y) {
                    include(_evalCubic(cur.x, c1.x, c2.x, p.x, t), _evalCubic(cur.y, c1.y, c2.y, p.y, t))
                }
                cur = p
            case .closeSubpath:
                cur = subStart
            }
        }
        guard minX.isFinite && minY.isFinite else { return .null }
        return CGRect(x: minX, y: minY, width: maxX - minX, height: maxY - minY)
    }

    public func contains(_ p: CGPoint, eoFill: Bool = false) -> Bool {
        let polys = _flattenClosed(tolerance: 0.25)
        return _polysContain(polys, p, eoFill: eoFill)
    }

    // MARK: - Mutating builders

    public mutating func move(to end: CGPoint) {
        elements.append(.move(to: end))
    }

    public mutating func addLine(to end: CGPoint) {
        if currentPoint == nil { elements.append(.move(to: .zero)) }
        elements.append(.line(to: end))
    }

    public mutating func addQuadCurve(to end: CGPoint, control: CGPoint) {
        if currentPoint == nil { elements.append(.move(to: .zero)) }
        elements.append(.quadCurve(to: end, control: control))
    }

    public mutating func addCurve(to end: CGPoint, control1: CGPoint, control2: CGPoint) {
        if currentPoint == nil { elements.append(.move(to: .zero)) }
        elements.append(.curve(to: end, control1: control1, control2: control2))
    }

    public mutating func closeSubpath() {
        elements.append(.closeSubpath)
    }

    public mutating func addRect(_ rect: CGRect, transform: CGAffineTransform = .identity) {
        let r = rect.standardized
        guard !r.isNull else { return }
        var sub = Path()
        sub.move(to: CGPoint(x: r.minX, y: r.minY))
        sub.addLine(to: CGPoint(x: r.maxX, y: r.minY))
        sub.addLine(to: CGPoint(x: r.maxX, y: r.maxY))
        sub.addLine(to: CGPoint(x: r.minX, y: r.maxY))
        sub.closeSubpath()
        addPath(sub, transform: transform)
    }

    public mutating func addRects(_ rects: [CGRect], transform: CGAffineTransform = .identity) {
        for r in rects { addRect(r, transform: transform) }
    }

    public mutating func addLines(_ lines: [CGPoint]) {
        guard let first = lines.first else { return }
        move(to: first)
        for p in lines.dropFirst() { addLine(to: p) }
    }

    public mutating func addEllipse(in rect: CGRect, transform: CGAffineTransform = .identity) {
        let r = rect.standardized
        guard !r.isNull, r.width > 0 || r.height > 0 else { return }
        let cx = r.midX, cy = r.midY, rx = r.width / 2, ry = r.height / 2
        let k: CGFloat = 0.5522847498307936
        let kx = rx * k, ky = ry * k
        var sub = Path()
        // Matches Apple SwiftUI: starts at 3 o'clock (maxX, midY) and sweeps clockwise on screen.
        sub.move(to: CGPoint(x: cx + rx, y: cy))
        sub.addCurve(to: CGPoint(x: cx, y: cy + ry), control1: CGPoint(x: cx + rx, y: cy + ky), control2: CGPoint(x: cx + kx, y: cy + ry))
        sub.addCurve(to: CGPoint(x: cx - rx, y: cy), control1: CGPoint(x: cx - kx, y: cy + ry), control2: CGPoint(x: cx - rx, y: cy + ky))
        sub.addCurve(to: CGPoint(x: cx, y: cy - ry), control1: CGPoint(x: cx - rx, y: cy - ky), control2: CGPoint(x: cx - kx, y: cy - ry))
        sub.addCurve(to: CGPoint(x: cx + rx, y: cy), control1: CGPoint(x: cx + kx, y: cy - ry), control2: CGPoint(x: cx + rx, y: cy - ky))
        sub.closeSubpath()
        addPath(sub, transform: transform)
    }

    public mutating func addRoundedRect(
        in rect: CGRect,
        cornerSize: CGSize,
        style: RoundedCornerStyle = .continuous,
        transform: CGAffineTransform = .identity
    ) {
        let rx = max(0, cornerSize.width), ry = max(0, cornerSize.height)
        addPath(_roundedRectPath(in: rect, radii: [(rx, ry), (rx, ry), (rx, ry), (rx, ry)], style: style), transform: transform)
    }

    public mutating func addRoundedRect(
        in rect: CGRect,
        cornerRadii: RectangleCornerRadii,
        style: RoundedCornerStyle = .continuous,
        transform: CGAffineTransform = .identity
    ) {
        let tl = max(0, cornerRadii.topLeading), tr = max(0, cornerRadii.topTrailing)
        let br = max(0, cornerRadii.bottomTrailing), bl = max(0, cornerRadii.bottomLeading)
        addPath(_roundedRectPath(in: rect, radii: [(tl, tl), (tr, tr), (br, br), (bl, bl)], style: style), transform: transform)
    }

    public mutating func addRelativeArc(
        center: CGPoint,
        radius: CGFloat,
        startAngle: Angle,
        delta: Angle,
        transform: CGAffineTransform = .identity
    ) {
        let r = abs(radius)
        let a0 = startAngle.radians
        let start = CGPoint(x: center.x + r * CGFloat(cos(a0)), y: center.y + r * CGFloat(sin(a0))).applying(transform)
        if currentPoint == nil {
            move(to: start)
        } else {
            addLine(to: start)
        }
        let total = delta.radians
        guard abs(total) > 1e-12 else { return }
        let segments = max(1, Int(ceil(abs(total) / (.pi / 2))))
        let step = total / Double(segments)
        let alpha = 4.0 / 3.0 * tan(step / 4.0)
        for i in 0..<segments {
            let t0 = a0 + Double(i) * step
            let t1 = t0 + step
            let (c0, s0) = (cos(t0), sin(t0))
            let (c1, s1) = (cos(t1), sin(t1))
            let cp1 = CGPoint(x: center.x + r * CGFloat(c0 - alpha * s0), y: center.y + r * CGFloat(s0 + alpha * c0)).applying(transform)
            let cp2 = CGPoint(x: center.x + r * CGFloat(c1 + alpha * s1), y: center.y + r * CGFloat(s1 - alpha * c1)).applying(transform)
            let end = CGPoint(x: center.x + r * CGFloat(c1), y: center.y + r * CGFloat(s1)).applying(transform)
            addCurve(to: end, control1: cp1, control2: cp2)
        }
    }

    public mutating func addArc(
        center: CGPoint,
        radius: CGFloat,
        startAngle: Angle,
        endAngle: Angle,
        clockwise: Bool,
        transform: CGAffineTransform = .identity
    ) {
        // SwiftUI matches CoreGraphics: clockwise=false increases angle (visually CW in y-down),
        // clockwise=true decreases angle (visually CCW in y-down).
        var d = endAngle.radians - startAngle.radians
        let tau = 2.0 * Double.pi
        if clockwise {
            while d > 0 { d -= tau }
            while d <= -tau { d += tau }
            if d == 0 && endAngle.radians != startAngle.radians { d = -tau }
        } else {
            while d < 0 { d += tau }
            while d >= tau { d -= tau }
            if d == 0 && endAngle.radians != startAngle.radians { d = tau }
        }
        addRelativeArc(center: center, radius: radius, startAngle: startAngle, delta: .radians(d), transform: transform)
    }

    public mutating func addArc(
        tangent1End p1: CGPoint,
        tangent2End p2: CGPoint,
        radius: CGFloat,
        transform: CGAffineTransform = .identity
    ) {
        let p0 = currentPoint ?? .zero
        // Un-transform current point into local space so tangent math matches CGPathAddArcToPoint.
        let inv = transform.inverted()
        let lp0 = p0.applying(inv)
        let v1x = lp0.x - p1.x, v1y = lp0.y - p1.y
        let v2x = p2.x - p1.x, v2y = p2.y - p1.y
        let len1 = CGFloat(hypot(Double(v1x), Double(v1y)))
        let len2 = CGFloat(hypot(Double(v2x), Double(v2y)))
        guard len1 > 1e-9, len2 > 1e-9, radius > 1e-9 else {
            addLine(to: p1.applying(transform))
            return
        }
        let u1x = v1x / len1, u1y = v1y / len1
        let u2x = v2x / len2, u2y = v2y / len2
        let dot = max(-1.0, min(1.0, Double(u1x * u2x + u1y * u2y)))
        let cross = Double(u1x * u2y - u1y * u2x)
        guard abs(cross) > 1e-9 else {
            addLine(to: p1.applying(transform))
            return
        }
        let halfAngle = acos(dot) / 2.0
        let dist = radius / CGFloat(tan(halfAngle))
        let t1 = CGPoint(x: p1.x + u1x * dist, y: p1.y + u1y * dist)
        let t2 = CGPoint(x: p1.x + u2x * dist, y: p1.y + u2y * dist)
        // Normal pointing from t1 toward center.
        let sign: CGFloat = cross > 0 ? 1 : -1
        let cx = t1.x - u1y * radius * sign
        let cy = t1.y + u1x * radius * sign
        let a1 = atan2(Double(t1.y - cy), Double(t1.x - cx))
        let a2 = atan2(Double(t2.y - cy), Double(t2.x - cx))
        var delta = a2 - a1
        if sign > 0 {
            while delta > 0 { delta -= 2 * .pi }
        } else {
            while delta < 0 { delta += 2 * .pi }
        }
        addRelativeArc(center: CGPoint(x: cx, y: cy), radius: radius, startAngle: .radians(a1), delta: .radians(delta), transform: transform)
    }

    public mutating func addPath(_ path: Path, transform: CGAffineTransform = .identity) {
        if transform.isIdentity {
            elements.append(contentsOf: path.elements)
        } else {
            for e in path.elements {
                switch e {
                case .move(let p): elements.append(.move(to: p.applying(transform)))
                case .line(let p): elements.append(.line(to: p.applying(transform)))
                case .quadCurve(let p, let c): elements.append(.quadCurve(to: p.applying(transform), control: c.applying(transform)))
                case .curve(let p, let c1, let c2):
                    elements.append(.curve(to: p.applying(transform), control1: c1.applying(transform), control2: c2.applying(transform)))
                case .closeSubpath: elements.append(.closeSubpath)
                }
            }
        }
    }

    // MARK: - Transforms

    public func applying(_ transform: CGAffineTransform) -> Path {
        if transform.isIdentity { return self }
        var out = Path()
        out.addPath(self, transform: transform)
        return out
    }

    public func offsetBy(dx: CGFloat, dy: CGFloat) -> Path {
        applying(CGAffineTransform(translationX: dx, y: dy))
    }

    // MARK: - Trimming

    public func trimmedPath(from startFraction: CGFloat, to endFraction: CGFloat) -> Path {
        let f0 = max(0, min(1, startFraction))
        let f1 = max(0, min(1, endFraction))
        if f0 >= f1 || elements.isEmpty { return Path() }
        if f0 <= 0 && f1 >= 1 { return self }

        let segs = _segments()
        var lengths: [CGFloat] = []
        lengths.reserveCapacity(segs.count)
        var total: CGFloat = 0
        for s in segs {
            let l = s.arcLength()
            lengths.append(l)
            total += l
        }
        guard total > 1e-12 else { return Path() }
        let d0 = f0 * total
        let d1 = f1 * total
        var out = Path()
        var acc: CGFloat = 0
        var lastEnd: CGPoint?
        for (s, len) in zip(segs, lengths) {
            let nextAcc = acc + len
            if nextAcc > d0 && acc < d1 && len > 1e-12 {
                let u0 = max(0, (d0 - acc) / len)
                let u1 = min(1, (d1 - acc) / len)
                let t0 = s.tForLengthFraction(u0)
                let t1 = s.tForLengthFraction(u1)
                let sub = s.subsegment(from: t0, to: t1)
                let sp = sub.start
                if let le = lastEnd, abs(le.x - sp.x) < 1e-5 && abs(le.y - sp.y) < 1e-5 {
                    // continue current subpath
                } else {
                    out.move(to: sp)
                }
                sub.appendTo(&out)
                lastEnd = sub.end
            }
            acc = nextAcc
        }
        return out
    }

    // MARK: - Stroking

    public func strokedPath(_ style: StrokeStyle) -> Path {
        let lw = max(0, style.lineWidth)
        guard lw > 0, !elements.isEmpty else { return Path() }
        let polylines = _flattenPolylines(tolerance: 0.25)
        let dashed: [([CGPoint], Bool)]
        let dashLens = style.dash.filter { $0 >= 0 }
        let dashSum = dashLens.reduce(0, +)
        if !dashLens.isEmpty && dashSum > 1e-9 {
            let pat = dashLens.count % 2 == 1 ? dashLens + dashLens : dashLens
            dashed = polylines.flatMap { _dashPolyline($0.0, closed: $0.1, pattern: pat, phase: style.dashPhase) }
        } else {
            dashed = polylines
        }
        var out = Path()
        for (pts, closed) in dashed {
            let outline = _strokePolyline(pts, closed: closed, halfWidth: lw / 2, style: style)
            out.addPath(outline)
        }
        return out
    }

    // MARK: - Boolean operations

    public func normalized(eoFill: Bool = true) -> Path {
        _booleanOp(self, Path(), op: .normalized, eoFill: eoFill)
    }

    public func union(_ other: Path, eoFill: Bool = false) -> Path {
        _booleanOp(self, other, op: .union, eoFill: eoFill)
    }

    public func intersection(_ other: Path, eoFill: Bool = false) -> Path {
        _booleanOp(self, other, op: .intersection, eoFill: eoFill)
    }

    public func subtracting(_ other: Path, eoFill: Bool = false) -> Path {
        _booleanOp(self, other, op: .subtracting, eoFill: eoFill)
    }

    public func symmetricDifference(_ other: Path, eoFill: Bool = false) -> Path {
        _booleanOp(self, other, op: .symmetricDifference, eoFill: eoFill)
    }

    public func lineIntersection(_ other: Path, eoFill: Bool = false) -> Path {
        _lineClip(self, against: other, keepInside: true, eoFill: eoFill)
    }

    public func lineSubtraction(_ other: Path, eoFill: Bool = false) -> Path {
        _lineClip(self, against: other, keepInside: false, eoFill: eoFill)
    }

    // MARK: - Wire encoding

    func _wireOps() -> [Any] {
        elements.map { e -> Any in
            switch e {
            case .move(let p): return ["m": [Double(p.x), Double(p.y)]]
            case .line(let p): return ["l": [Double(p.x), Double(p.y)]]
            case .quadCurve(let p, let c): return ["q": [Double(c.x), Double(c.y), Double(p.x), Double(p.y)]]
            case .curve(let p, let c1, let c2):
                return ["c": [Double(c1.x), Double(c1.y), Double(c2.x), Double(c2.y), Double(p.x), Double(p.y)]]
            case .closeSubpath: return "z"
            }
        }
    }
}

#if canImport(CoreGraphics)
extension Path {
    public init(_ path: CGPath) {
        var elems: [Element] = []
        path.applyWithBlock { ptr in
            let el = ptr.pointee
            let pts = el.points
            switch el.type {
            case .moveToPoint: elems.append(.move(to: pts[0]))
            case .addLineToPoint: elems.append(.line(to: pts[0]))
            case .addQuadCurveToPoint: elems.append(.quadCurve(to: pts[1], control: pts[0]))
            case .addCurveToPoint: elems.append(.curve(to: pts[2], control1: pts[0], control2: pts[1]))
            case .closeSubpath: elems.append(.closeSubpath)
            @unknown default: break
            }
        }
        self.elements = elems
    }

    public init(_ path: CGMutablePath) {
        self.init(path as CGPath)
    }

    public var cgPath: CGPath {
        let m = CGMutablePath()
        for e in elements {
            switch e {
            case .move(let p): m.move(to: p)
            case .line(let p): m.addLine(to: p)
            case .quadCurve(let p, let c): m.addQuadCurve(to: p, control: c)
            case .curve(let p, let c1, let c2): m.addCurve(to: p, control1: c1, control2: c2)
            case .closeSubpath: m.closeSubpath()
            }
        }
        return m.copy() ?? m
    }
}
#endif

// MARK: - Rounded rect path builder (matches Apple SwiftUI circular & continuous squircle)

func _roundedRectPath(in rect: CGRect, radii raw: [(CGFloat, CGFloat)], style: RoundedCornerStyle) -> Path {
    let r = rect.standardized
    guard !r.isNull, r.width > 0 || r.height > 0 else { return Path() }
    let x = r.minX, y = r.minY, w = r.width, h = r.height
    var cr = raw.map { (max(0, $0.0), max(0, $0.1)) }
    var scale: CGFloat = 1
    func clampPair(_ sum: CGFloat, _ len: CGFloat) {
        if sum > len && sum > 0 { scale = min(scale, len / sum) }
    }
    clampPair(cr[0].0 + cr[1].0, w)
    clampPair(cr[3].0 + cr[2].0, w)
    clampPair(cr[0].1 + cr[3].1, h)
    clampPair(cr[1].1 + cr[2].1, h)
    if scale < 1 { cr = cr.map { ($0.0 * scale, $0.1 * scale) } }
    let (tl, tr, br, bl) = (cr[0], cr[1], cr[2], cr[3])
    let k: CGFloat = 0.5522847498307936
    let continuous = style == .continuous

    func budget(_ a: CGFloat, _ b: CGFloat, _ len: CGFloat) -> (CGFloat, CGFloat) {
        a + b <= 1e-6 ? (len * 0.5, len * 0.5) : (len * a / (a + b), len * b / (a + b))
    }
    let (topL, topR) = budget(tl.0, tr.0, w)
    let (botL, botR) = budget(bl.0, br.0, w)
    let (leftT, leftB) = budget(tl.1, bl.1, h)
    let (rightT, rightB) = budget(tr.1, br.1, h)

    var p = Path()
    // Start on right edge (matches Apple SwiftUI start at maxX, midY -> BR -> BL -> TL -> TR).
    let rightStartY = y + rightT
    p.move(to: CGPoint(x: x + w, y: rightStartY))

    func emitCorner(
        cx: CGFloat, cy: CGFloat,
        ux: CGFloat, uy: CGFloat,
        vx: CGFloat, vy: CGFloat,
        ru: CGFloat, rv: CGFloat,
        au: CGFloat, av: CGFloat
    ) {
        func pt(_ du: CGFloat, _ dv: CGFloat) -> CGPoint {
            CGPoint(x: cx + ux * du + vx * dv, y: cy + uy * du + vy * dv)
        }
        if ru <= 1e-4 || rv <= 1e-4 {
            p.addLine(to: pt(0, 0))
            return
        }
        let P: CGFloat = 1.52866
        if continuous && au >= ru * P - 1e-3 && av >= rv * P - 1e-3 {
            p.addLine(to: pt(ru * P, 0))
            p.addCurve(to: pt(ru * 0.631494, rv * 0.0749114), control1: pt(ru * 1.08849, 0), control2: pt(ru * 0.868407, 0))
            p.addCurve(to: pt(ru * 0.0749114, rv * 0.631494), control1: pt(ru * 0.372824, rv * 0.16906), control2: pt(ru * 0.16906, rv * 0.372824))
            p.addCurve(to: pt(0, rv * P), control1: pt(0, rv * 0.868407), control2: pt(0, rv * 1.08849))
        } else {
            p.addLine(to: pt(ru, 0))
            p.addCurve(to: pt(0, rv), control1: pt(ru * (1 - k), 0), control2: pt(0, rv * (1 - k)))
        }
    }

    // Bottom-right
    emitCorner(cx: x + w, cy: y + h, ux: 0, uy: -1, vx: -1, vy: 0, ru: br.1, rv: br.0, au: rightB, av: botR)
    // Bottom-left
    emitCorner(cx: x, cy: y + h, ux: 1, uy: 0, vx: 0, vy: -1, ru: bl.0, rv: bl.1, au: botL, av: leftB)
    // Top-left
    emitCorner(cx: x, cy: y, ux: 0, uy: 1, vx: 1, vy: 0, ru: tl.1, rv: tl.0, au: leftT, av: topL)
    // Top-right
    emitCorner(cx: x + w, cy: y, ux: -1, uy: 0, vx: 0, vy: 1, ru: tr.0, rv: tr.1, au: topR, av: rightT)
    p.closeSubpath()
    return p
}

// MARK: - Bezier math helpers

private func _evalQuad(_ p0: CGFloat, _ p1: CGFloat, _ p2: CGFloat, _ t: CGFloat) -> CGFloat {
    let mt = 1 - t
    return mt * mt * p0 + 2 * mt * t * p1 + t * t * p2
}

private func _evalCubic(_ p0: CGFloat, _ p1: CGFloat, _ p2: CGFloat, _ p3: CGFloat, _ t: CGFloat) -> CGFloat {
    let mt = 1 - t
    return mt * mt * mt * p0 + 3 * mt * mt * t * p1 + 3 * mt * t * t * p2 + t * t * t * p3
}

private func _quadExtrema(_ p0: CGFloat, _ p1: CGFloat, _ p2: CGFloat) -> [CGFloat] {
    let d = p0 - 2 * p1 + p2
    guard abs(d) > 1e-12 else { return [] }
    let t = (p0 - p1) / d
    return (t > 0 && t < 1) ? [t] : []
}

private func _cubicExtrema(_ p0: CGFloat, _ p1: CGFloat, _ p2: CGFloat, _ p3: CGFloat) -> [CGFloat] {
    let a = -p0 + 3 * p1 - 3 * p2 + p3
    let b = 2 * (p0 - 2 * p1 + p2)
    let c = p1 - p0
    var roots: [CGFloat] = []
    if abs(a) < 1e-12 {
        if abs(b) > 1e-12 {
            let t = -c / b
            if t > 0 && t < 1 { roots.append(t) }
        }
        return roots
    }
    let disc = b * b - 4 * a * c
    guard disc >= 0 else { return [] }
    let sd = CGFloat(sqrt(Double(disc)))
    for t in [(-b - sd) / (2 * a), (-b + sd) / (2 * a)] where t > 0 && t < 1 {
        roots.append(t)
    }
    return roots
}

private func _lerp(_ a: CGPoint, _ b: CGPoint, _ t: CGFloat) -> CGPoint {
    CGPoint(x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t)
}

private func _dist(_ a: CGPoint, _ b: CGPoint) -> CGFloat {
    CGFloat(hypot(Double(b.x - a.x), Double(b.y - a.y)))
}

// MARK: - Path segments for trimming

private enum _Seg {
    case line(CGPoint, CGPoint)
    case quad(CGPoint, CGPoint, CGPoint)
    case cubic(CGPoint, CGPoint, CGPoint, CGPoint)

    var start: CGPoint {
        switch self {
        case .line(let a, _), .quad(let a, _, _), .cubic(let a, _, _, _): return a
        }
    }
    var end: CGPoint {
        switch self {
        case .line(_, let b), .quad(_, _, let b), .cubic(_, _, _, let b): return b
        }
    }

    func point(at t: CGFloat) -> CGPoint {
        switch self {
        case .line(let a, let b): return _lerp(a, b, t)
        case .quad(let a, let c, let b):
            return CGPoint(x: _evalQuad(a.x, c.x, b.x, t), y: _evalQuad(a.y, c.y, b.y, t))
        case .cubic(let a, let c1, let c2, let b):
            return CGPoint(x: _evalCubic(a.x, c1.x, c2.x, b.x, t), y: _evalCubic(a.y, c1.y, c2.y, b.y, t))
        }
    }

    func arcLength(steps: Int = 32) -> CGFloat {
        if case .line(let a, let b) = self { return _dist(a, b) }
        var total: CGFloat = 0
        var prev = start
        for i in 1...steps {
            let p = point(at: CGFloat(i) / CGFloat(steps))
            total += _dist(prev, p)
            prev = p
        }
        return total
    }

    func tForLengthFraction(_ frac: CGFloat, steps: Int = 32) -> CGFloat {
        if frac <= 0 { return 0 }
        if frac >= 1 { return 1 }
        if case .line = self { return frac }
        var cum: [CGFloat] = [0]
        cum.reserveCapacity(steps + 1)
        var prev = start
        var total: CGFloat = 0
        for i in 1...steps {
            let p = point(at: CGFloat(i) / CGFloat(steps))
            total += _dist(prev, p)
            cum.append(total)
            prev = p
        }
        guard total > 1e-12 else { return frac }
        let target = frac * total
        for i in 1...steps where cum[i] >= target {
            let span = cum[i] - cum[i - 1]
            let local = span > 1e-12 ? (target - cum[i - 1]) / span : 0
            return (CGFloat(i - 1) + local) / CGFloat(steps)
        }
        return 1
    }

    func splitRight(from t: CGFloat) -> _Seg {
        switch self {
        case .line(let a, let b): return .line(_lerp(a, b, t), b)
        case .quad(let a, let c, let b):
            let p01 = _lerp(a, c, t), p12 = _lerp(c, b, t), p012 = _lerp(p01, p12, t)
            return .quad(p012, p12, b)
        case .cubic(let a, let c1, let c2, let b):
            let p01 = _lerp(a, c1, t), p12 = _lerp(c1, c2, t), p23 = _lerp(c2, b, t)
            let p012 = _lerp(p01, p12, t), p123 = _lerp(p12, p23, t), p0123 = _lerp(p012, p123, t)
            return .cubic(p0123, p123, p23, b)
        }
    }

    func splitLeft(to t: CGFloat) -> _Seg {
        switch self {
        case .line(let a, let b): return .line(a, _lerp(a, b, t))
        case .quad(let a, let c, let b):
            let p01 = _lerp(a, c, t), p12 = _lerp(c, b, t), p012 = _lerp(p01, p12, t)
            return .quad(a, p01, p012)
        case .cubic(let a, let c1, let c2, let b):
            let p01 = _lerp(a, c1, t), p12 = _lerp(c1, c2, t), p23 = _lerp(c2, b, t)
            let p012 = _lerp(p01, p12, t), p123 = _lerp(p12, p23, t), p0123 = _lerp(p012, p123, t)
            return .cubic(a, p01, p012, p0123)
        }
    }

    func subsegment(from t0: CGFloat, to t1: CGFloat) -> _Seg {
        let right = t0 > 0 ? splitRight(from: t0) : self
        let rem = 1 - t0
        let rel = rem > 1e-12 ? min(1, max(0, (t1 - t0) / rem)) : 1
        return rel < 1 ? right.splitLeft(to: rel) : right
    }

    func appendTo(_ p: inout Path) {
        switch self {
        case .line(_, let b): p.addLine(to: b)
        case .quad(_, let c, let b): p.addQuadCurve(to: b, control: c)
        case .cubic(_, let c1, let c2, let b): p.addCurve(to: b, control1: c1, control2: c2)
        }
    }
}

extension Path {
    fileprivate func _segments() -> [_Seg] {
        var out: [_Seg] = []
        var cur = CGPoint.zero
        var subStart = CGPoint.zero
        var hasCurrent = false
        for e in elements {
            switch e {
            case .move(let p): cur = p; subStart = p; hasCurrent = true
            case .line(let p):
                if !hasCurrent { cur = .zero; subStart = .zero; hasCurrent = true }
                out.append(.line(cur, p)); cur = p
            case .quadCurve(let p, let c):
                if !hasCurrent { cur = .zero; subStart = .zero; hasCurrent = true }
                out.append(.quad(cur, c, p)); cur = p
            case .curve(let p, let c1, let c2):
                if !hasCurrent { cur = .zero; subStart = .zero; hasCurrent = true }
                out.append(.cubic(cur, c1, c2, p)); cur = p
            case .closeSubpath:
                if hasCurrent && _dist(cur, subStart) > 1e-9 {
                    out.append(.line(cur, subStart))
                }
                cur = subStart
            }
        }
        return out
    }

    /// Flatten every subpath into `(points, isClosed)` polylines with flatness error <= `tolerance`.
    fileprivate func _flattenPolylines(tolerance: CGFloat) -> [([CGPoint], Bool)] {
        var out: [([CGPoint], Bool)] = []
        var pts: [CGPoint] = []
        var cur = CGPoint.zero
        var subStart = CGPoint.zero

        func flush(closed: Bool) {
            if pts.count >= 2 {
                if closed, let f = pts.first, let l = pts.last, _dist(f, l) < 1e-7 {
                    pts.removeLast()
                }
                if pts.count >= 2 { out.append((pts, closed)) }
            }
            pts = []
        }

        func flattenQuad(_ p0: CGPoint, _ c: CGPoint, _ p1: CGPoint, _ depth: Int) {
            let mx = (p0.x + p1.x) * 0.5, my = (p0.y + p1.y) * 0.5
            let err = hypot(Double(c.x - mx), Double(c.y - my)) * 0.5
            if err <= Double(tolerance) || depth >= 10 {
                pts.append(p1); return
            }
            let a = _lerp(p0, c, 0.5), b = _lerp(c, p1, 0.5), m = _lerp(a, b, 0.5)
            flattenQuad(p0, a, m, depth + 1)
            flattenQuad(m, b, p1, depth + 1)
        }

        func flattenCubic(_ p0: CGPoint, _ c1: CGPoint, _ c2: CGPoint, _ p1: CGPoint, _ depth: Int) {
            let dx = p1.x - p0.x, dy = p1.y - p0.y
            let chordSquared = dx * dx + dy * dy
            func distanceToChord(_ control: CGPoint) -> CGFloat {
                guard chordSquared > 1e-18 else { return _dist(p0, control) }
                let projection = ((control.x - p0.x) * dx + (control.y - p0.y) * dy) / chordSquared
                return _dist(control, _lerp(p0, p1, min(1, max(0, projection))))
            }
            if depth >= 10 || max(distanceToChord(c1), distanceToChord(c2)) <= tolerance {
                pts.append(p1); return
            }
            let p01 = _lerp(p0, c1, 0.5), p12 = _lerp(c1, c2, 0.5), p23 = _lerp(c2, p1, 0.5)
            let p012 = _lerp(p01, p12, 0.5), p123 = _lerp(p12, p23, 0.5), m = _lerp(p012, p123, 0.5)
            flattenCubic(p0, p01, p012, m, depth + 1)
            flattenCubic(m, p123, p23, p1, depth + 1)
        }

        for e in elements {
            switch e {
            case .move(let p):
                flush(closed: false)
                cur = p; subStart = p; pts = [p]
            case .line(let p):
                if pts.isEmpty { cur = .zero; subStart = .zero; pts = [cur] }
                if _dist(cur, p) > 1e-9 { pts.append(p) }
                cur = p
            case .quadCurve(let p, let c):
                if pts.isEmpty { cur = .zero; subStart = .zero; pts = [cur] }
                flattenQuad(cur, c, p, 0); cur = p
            case .curve(let p, let c1, let c2):
                if pts.isEmpty { cur = .zero; subStart = .zero; pts = [cur] }
                flattenCubic(cur, c1, c2, p, 0); cur = p
            case .closeSubpath:
                flush(closed: true); cur = subStart
            }
        }
        flush(closed: false)
        return out
    }

    fileprivate func _flattenClosed(tolerance: CGFloat) -> [[CGPoint]] {
        _flattenPolylines(tolerance: tolerance).compactMap { pts, _ in
            pts.count >= 3 ? pts : nil
        }
    }
}

// MARK: - Point-in-polygon (winding & even-odd)

private func _polysContain(_ polys: [[CGPoint]], _ p: CGPoint, eoFill: Bool) -> Bool {
    var winding = 0
    var crossings = 0
    for poly in polys {
        let n = poly.count
        guard n >= 3 else { continue }
        for i in 0..<n {
            let a = poly[i], b = poly[(i + 1) % n]
            if a.y <= p.y {
                if b.y > p.y {
                    let side = (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y)
                    if side > 0 { winding += 1; crossings += 1 }
                }
            } else if b.y <= p.y {
                let side = (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y)
                if side < 0 { winding -= 1; crossings += 1 }
            }
        }
    }
    return eoFill ? (crossings % 2 != 0) : (winding != 0)
}

// MARK: - Dashing & Stroke Expansion

private func _dashPolyline(_ pts: [CGPoint], closed: Bool, pattern: [CGFloat], phase: CGFloat) -> [([CGPoint], Bool)] {
    let n = pts.count
    guard n >= 2 else { return [] }
    let edgeCount = closed ? n : n - 1
    let patTotal = pattern.reduce(0, +)
    guard patTotal > 1e-9 else { return [(pts, closed)] }
    var ph = phase.truncatingRemainder(dividingBy: patTotal)
    if ph < 0 { ph += patTotal }
    var patIdx = 0
    while ph >= pattern[patIdx] && pattern[patIdx] > 0 {
        ph -= pattern[patIdx]
        patIdx = (patIdx + 1) % pattern.count
    }
    var rem = pattern[patIdx] - ph
    var drawing = (patIdx % 2 == 0)

    var out: [([CGPoint], Bool)] = []
    var current: [CGPoint] = drawing ? [pts[0]] : []
    for i in 0..<edgeCount {
        var a = pts[i]
        let b = pts[(i + 1) % n]
        var segLen = _dist(a, b)
        while segLen > 1e-9 {
            if rem <= segLen + 1e-9 {
                let t = min(1, rem / segLen)
                let mid = _lerp(a, b, t)
                if drawing {
                    current.append(mid)
                    if current.count >= 2 { out.append((current, false)) }
                    current = []
                }
                a = mid
                segLen -= rem
                patIdx = (patIdx + 1) % pattern.count
                rem = pattern[patIdx]
                drawing = (patIdx % 2 == 0)
                if drawing { current = [a] }
            } else {
                if drawing { current.append(b) }
                rem -= segLen
                segLen = 0
            }
        }
    }
    if drawing && current.count >= 2 { out.append((current, false)) }
    return out
}

private func _strokePolyline(_ raw: [CGPoint], closed: Bool, halfWidth hw: CGFloat, style: StrokeStyle) -> Path {
    // Filter consecutive duplicate points.
    var pts: [CGPoint] = []
    for p in raw {
        if let l = pts.last, _dist(l, p) < 1e-6 { continue }
        pts.append(p)
    }
    if closed, let f = pts.first, let l = pts.last, _dist(f, l) < 1e-6 { pts.removeLast() }
    guard pts.count >= 2 else { return Path() }

    func unit(_ a: CGPoint, _ b: CGPoint) -> CGPoint {
        let d = _dist(a, b)
        return d > 1e-9 ? CGPoint(x: (b.x - a.x) / d, y: (b.y - a.y) / d) : CGPoint(x: 1, y: 0)
    }
    func leftNorm(_ u: CGPoint) -> CGPoint { CGPoint(x: -u.y, y: u.x) }

    func joinPts(vertex v: CGPoint, n1: CGPoint, n2: CGPoint, side: CGFloat) -> [CGPoint] {
        // side = +1 for left side, -1 for right side
        let a = CGPoint(x: v.x + n1.x * hw * side, y: v.y + n1.y * hw * side)
        let b = CGPoint(x: v.x + n2.x * hw * side, y: v.y + n2.y * hw * side)
        let cross = n1.x * n2.y - n1.y * n2.x
        let isOuter = (cross * side) < -1e-6
        guard isOuter else { return [a, b] }
        switch style.lineJoin {
        case .bevel:
            return [a, b]
        case .round:
            let a1 = atan2(Double(n1.y * side), Double(n1.x * side))
            let a2 = atan2(Double(n2.y * side), Double(n2.x * side))
            var d = a2 - a1
            if side > 0 { while d > 0 { d -= 2 * .pi } } else { while d < 0 { d += 2 * .pi } }
            let steps = max(2, Int(ceil(abs(d) / (.pi / 8))))
            return (0...steps).map { i in
                let ang = a1 + d * Double(i) / Double(steps)
                return CGPoint(x: v.x + hw * CGFloat(cos(ang)), y: v.y + hw * CGFloat(sin(ang)))
            }
        case .miter:
            let dot = Double(n1.x * n2.x + n1.y * n2.y)
            let denom = 1.0 + dot
            guard denom > 1e-6 else { return [a, b] }
            let k = CGFloat(1.0 / denom)
            let mx = (n1.x + n2.x) * k * hw * side
            let my = (n1.y + n2.y) * k * hw * side
            let miterLen = CGFloat(hypot(Double(mx), Double(my)))
            if miterLen <= hw * max(1, style.miterLimit) {
                return [CGPoint(x: v.x + mx, y: v.y + my)]
            }
            return [a, b]
        #if canImport(CoreGraphics)
        @unknown default:
            return [a, b]
        #endif
        }
    }

    func capPts(end v: CGPoint, dir u: CGPoint) -> [CGPoint] {
        // u points outward from the end of the stroke.
        let n = leftNorm(u)
        let pL = CGPoint(x: v.x + n.x * hw, y: v.y + n.y * hw)
        let pR = CGPoint(x: v.x - n.x * hw, y: v.y - n.y * hw)
        switch style.lineCap {
        case .butt:
            return [pL, pR]
        case .square:
            return [
                CGPoint(x: pL.x + u.x * hw, y: pL.y + u.y * hw),
                CGPoint(x: pR.x + u.x * hw, y: pR.y + u.y * hw),
            ]
        case .round:
            let a0 = atan2(Double(n.y), Double(n.x))
            let steps = 8
            return (0...steps).map { i in
                let ang = a0 - Double.pi * Double(i) / Double(steps)
                return CGPoint(x: v.x + hw * CGFloat(cos(ang)), y: v.y + hw * CGFloat(sin(ang)))
            }
        #if canImport(CoreGraphics)
        @unknown default:
            return [pL, pR]
        #endif
        }
    }

    let n = pts.count
    if closed {
        var outer: [CGPoint] = []
        var inner: [CGPoint] = []
        for i in 0..<n {
            let prev = pts[(i + n - 1) % n], cur = pts[i], next = pts[(i + 1) % n]
            let n1 = leftNorm(unit(prev, cur)), n2 = leftNorm(unit(cur, next))
            outer.append(contentsOf: joinPts(vertex: cur, n1: n1, n2: n2, side: 1))
            inner.append(contentsOf: joinPts(vertex: cur, n1: n1, n2: n2, side: -1))
        }
        var p = Path()
        if let f = outer.first {
            p.move(to: f); for q in outer.dropFirst() { p.addLine(to: q) }; p.closeSubpath()
        }
        let rev = Array(inner.reversed())
        if let f = rev.first {
            p.move(to: f); for q in rev.dropFirst() { p.addLine(to: q) }; p.closeSubpath()
        }
        return p
    } else {
        var left: [CGPoint] = []
        var right: [CGPoint] = []
        let uStart = unit(pts[0], pts[1])
        let nStart = leftNorm(uStart)
        left.append(CGPoint(x: pts[0].x + nStart.x * hw, y: pts[0].y + nStart.y * hw))
        right.append(CGPoint(x: pts[0].x - nStart.x * hw, y: pts[0].y - nStart.y * hw))
        if n > 2 {
            for i in 1..<(n - 1) {
                let n1 = leftNorm(unit(pts[i - 1], pts[i]))
                let n2 = leftNorm(unit(pts[i], pts[i + 1]))
                left.append(contentsOf: joinPts(vertex: pts[i], n1: n1, n2: n2, side: 1))
                right.append(contentsOf: joinPts(vertex: pts[i], n1: n1, n2: n2, side: -1))
            }
        }
        let uEnd = unit(pts[n - 2], pts[n - 1])
        var loop = left
        loop.append(contentsOf: capPts(end: pts[n - 1], dir: uEnd))
        loop.append(contentsOf: right.reversed())
        loop.append(contentsOf: capPts(end: pts[0], dir: CGPoint(x: -uStart.x, y: -uStart.y)))
        var p = Path()
        if let f = loop.first {
            p.move(to: f); for q in loop.dropFirst() { p.addLine(to: q) }; p.closeSubpath()
        }
        return p
    }
}

// MARK: - Planar Subdivision Boolean Engine (union, intersection, subtracting, symmetricDifference, normalized, line*)

private enum _BoolOp { case union, intersection, subtracting, symmetricDifference, normalized }

private struct _Edge {
    let a: CGPoint, b: CGPoint
}

private struct _SegHit {
    let t: CGFloat?
    let u: CGFloat?
    let ts: [CGFloat]
    let us: [CGFloat]
    static let none = _SegHit(t: nil, u: nil, ts: [], us: [])
}

private func _polyEdges(_ polys: [[CGPoint]]) -> [_Edge] {
    var out: [_Edge] = []
    for poly in polys {
        for i in 0..<poly.count {
            let a = poly[i], b = poly[(i + 1) % poly.count]
            if _dist(a, b) > 1e-6 { out.append(_Edge(a: a, b: b)) }
        }
    }
    return out
}

private func _segParams(_ e1: _Edge, _ e2: _Edge) -> _SegHit {
    let dx1 = e1.b.x - e1.a.x, dy1 = e1.b.y - e1.a.y
    let dx2 = e2.b.x - e2.a.x, dy2 = e2.b.y - e2.a.y
    let denom = dx1 * dy2 - dy1 * dx2
    let rx = e2.a.x - e1.a.x, ry = e2.a.y - e1.a.y
    if abs(denom) > 1e-10 {
        let t = (rx * dy2 - ry * dx2) / denom
        let u = (rx * dy1 - ry * dx1) / denom
        if t >= -1e-7 && t <= 1 + 1e-7 && u >= -1e-7 && u <= 1 + 1e-7 {
            return _SegHit(t: max(0, min(1, t)), u: max(0, min(1, u)), ts: [], us: [])
        }
        return .none
    }
    // Parallel: check collinear overlap.
    let cross = rx * dy1 - ry * dx1
    let len1Sq = dx1 * dx1 + dy1 * dy1
    let len2Sq = dx2 * dx2 + dy2 * dy2
    guard len1Sq > 1e-12 && len2Sq > 1e-12 && abs(cross) <= 1e-6 * CGFloat(sqrt(Double(len1Sq))) else {
        return .none
    }
    var ts: [CGFloat] = []
    var us: [CGFloat] = []
    for p in [e2.a, e2.b] {
        let t = ((p.x - e1.a.x) * dx1 + (p.y - e1.a.y) * dy1) / len1Sq
        if t > 1e-7 && t < 1 - 1e-7 { ts.append(t) }
    }
    for p in [e1.a, e1.b] {
        let u = ((p.x - e2.a.x) * dx2 + (p.y - e2.a.y) * dy2) / len2Sq
        if u > 1e-7 && u < 1 - 1e-7 { us.append(u) }
    }
    return _SegHit(t: nil, u: nil, ts: ts, us: us)
}

private func _splitEdges(_ edges: [_Edge]) -> [_Edge] {
    let n = edges.count
    var splits = Array(repeating: [CGFloat](), count: n)
    // Precompute AABBs for quick rejection.
    let boxes: [(CGFloat, CGFloat, CGFloat, CGFloat)] = edges.map { e in
        (min(e.a.x, e.b.x) - 1e-5, max(e.a.x, e.b.x) + 1e-5, min(e.a.y, e.b.y) - 1e-5, max(e.a.y, e.b.y) + 1e-5)
    }
    for i in 0..<n {
        let bi = boxes[i]
        for j in (i + 1)..<n {
            let bj = boxes[j]
            if bi.1 < bj.0 || bj.1 < bi.0 || bi.3 < bj.2 || bj.3 < bi.2 { continue }
            let hit = _segParams(edges[i], edges[j])
            if let t = hit.t, t > 1e-7 && t < 1 - 1e-7 { splits[i].append(t) }
            if let u = hit.u, u > 1e-7 && u < 1 - 1e-7 { splits[j].append(u) }
            if !hit.ts.isEmpty { splits[i].append(contentsOf: hit.ts) }
            if !hit.us.isEmpty { splits[j].append(contentsOf: hit.us) }
        }
    }
    var out: [_Edge] = []
    out.reserveCapacity(n * 2)
    for i in 0..<n {
        let e = edges[i]
        if splits[i].isEmpty {
            if _dist(e.a, e.b) > 1e-6 { out.append(e) }
            continue
        }
        var ts = splits[i]
        ts.sort()
        var prevT: CGFloat = 0
        for t in ts {
            if t - prevT > 1e-7 {
                let p0 = _lerp(e.a, e.b, prevT), p1 = _lerp(e.a, e.b, t)
                if _dist(p0, p1) > 1e-6 { out.append(_Edge(a: p0, b: p1)) }
                prevT = t
            }
        }
        let p0 = _lerp(e.a, e.b, prevT)
        if _dist(p0, e.b) > 1e-6 { out.append(_Edge(a: p0, b: e.b)) }
    }
    return out
}

private struct _VKey: Hashable {
    let x: Int64, y: Int64
    init(_ p: CGPoint) {
        x = Int64((Double(p.x) * 1000.0).rounded())
        y = Int64((Double(p.y) * 1000.0).rounded())
    }
}

private struct _EKey: Hashable {
    let a: _VKey, b: _VKey
    init(_ p0: CGPoint, _ p1: CGPoint) {
        let k0 = _VKey(p0), k1 = _VKey(p1)
        if k0.x < k1.x || (k0.x == k1.x && k0.y <= k1.y) { a = k0; b = k1 } else { a = k1; b = k0 }
    }
}

private func _booleanOp(_ lhs: Path, _ rhs: Path, op: _BoolOp, eoFill: Bool) -> Path {
    let polysA = lhs._flattenClosed(tolerance: 0.25)
    let polysB = rhs._flattenClosed(tolerance: 0.25)
    if polysA.isEmpty && polysB.isEmpty { return Path() }

    let allEdges = _polyEdges(polysA) + _polyEdges(polysB)
    guard !allEdges.isEmpty else { return Path() }

    let pieces = _splitEdges(allEdges)

    func eval(_ a: Bool, _ b: Bool) -> Bool {
        switch op {
        case .union: return a || b
        case .intersection: return a && b
        case .subtracting: return a && !b
        case .symmetricDifference: return a != b
        case .normalized: return a
        }
    }

    var seen = Set<_EKey>()
    var kept: [_Edge] = []
    for e in pieces {
        let ek = _EKey(e.a, e.b)
        if seen.contains(ek) { continue }
        seen.insert(ek)
        let len = _dist(e.a, e.b)
        guard len > 1e-6 else { continue }
        let mx = (e.a.x + e.b.x) * 0.5, my = (e.a.y + e.b.y) * 0.5
        let eps = min(1e-3, len * 0.1)
        let nx = -(e.b.y - e.a.y) / len * eps
        let ny = (e.b.x - e.a.x) / len * eps
        let pL = CGPoint(x: mx + nx, y: my + ny)
        let pR = CGPoint(x: mx - nx, y: my - ny)
        let inAL = _polysContain(polysA, pL, eoFill: eoFill)
        let inAR = _polysContain(polysA, pR, eoFill: eoFill)
        let inBL = polysB.isEmpty ? false : _polysContain(polysB, pL, eoFill: eoFill)
        let inBR = polysB.isEmpty ? false : _polysContain(polysB, pR, eoFill: eoFill)
        let resL = eval(inAL, inBL)
        let resR = eval(inAR, inBR)
        if resL && !resR {
            kept.append(e)
        } else if !resL && resR {
            kept.append(_Edge(a: e.b, b: e.a))
        }
    }
    return _chainBoundary(kept)
}

private func _chainBoundary(_ edges: [_Edge]) -> Path {
    guard !edges.isEmpty else { return Path() }
    var outgoing: [_VKey: [Int]] = [:]
    for (i, e) in edges.enumerated() {
        outgoing[_VKey(e.a), default: []].append(i)
    }
    var used = Array(repeating: false, count: edges.count)
    var out = Path()

    for startIdx in 0..<edges.count where !used[startIdx] {
        var loop: [CGPoint] = [edges[startIdx].a]
        var curIdx = startIdx
        let startKey = _VKey(edges[startIdx].a)
        while true {
            used[curIdx] = true
            let e = edges[curIdx]
            let endPt = e.b
            let endKey = _VKey(endPt)
            if endKey == startKey { break }
            loop.append(endPt)
            guard let candidates = outgoing[endKey] else { break }
            let avail = candidates.filter { !used[$0] }
            guard !avail.isEmpty else { break }
            if avail.count == 1 {
                curIdx = avail[0]
            } else {
                // At a pinch vertex, pick the sharpest right turn so contours stay simple.
                let inAng = atan2(Double(e.a.y - e.b.y), Double(e.a.x - e.b.x))
                var best = avail[0], bestDelta = Double.infinity
                for c in avail {
                    let ce = edges[c]
                    let outAng = atan2(Double(ce.b.y - ce.a.y), Double(ce.b.x - ce.a.x))
                    var d = outAng - inAng
                    while d <= 0 { d += 2 * .pi }
                    if d < bestDelta { bestDelta = d; best = c }
                }
                curIdx = best
            }
        }
        guard loop.count >= 3 else { continue }
        // Simplify collinear consecutive points.
        var clean: [CGPoint] = []
        let m = loop.count
        for i in 0..<m {
            let prev = loop[(i + m - 1) % m], cur = loop[i], next = loop[(i + 1) % m]
            let cross = abs((cur.x - prev.x) * (next.y - cur.y) - (cur.y - prev.y) * (next.x - cur.x))
            if cross > 1e-5 { clean.append(cur) }
        }
        guard clean.count >= 3 else { continue }
        out.move(to: clean[0])
        for p in clean.dropFirst() { out.addLine(to: p) }
        out.closeSubpath()
    }
    return out
}

private func _lineClip(_ subject: Path, against clip: Path, keepInside: Bool, eoFill: Bool) -> Path {
    let polylines = subject._flattenPolylines(tolerance: 0.25)
    let clipPolys = clip._flattenClosed(tolerance: 0.25)
    guard !polylines.isEmpty else { return Path() }
    if clipPolys.isEmpty { return keepInside ? Path() : subject }

    let clipEdges = _polyEdges(clipPolys)

    var out = Path()
    for (pts, closed) in polylines {
        let count = closed ? pts.count : pts.count - 1
        guard count > 0 else { continue }
        var lastEnd: CGPoint?
        for i in 0..<count {
            let a = pts[i], b = pts[(i + 1) % pts.count]
            let e = _Edge(a: a, b: b)
            var ts: [CGFloat] = [0, 1]
            for ce in clipEdges {
                let hit = _segParams(e, ce)
                if let t = hit.t, t > 1e-7 && t < 1 - 1e-7 { ts.append(t) }
                ts.append(contentsOf: hit.ts)
            }
            ts.sort()
            for k in 0..<(ts.count - 1) {
                let t0 = ts[k], t1 = ts[k + 1]
                guard t1 - t0 > 1e-7 else { continue }
                let p0 = _lerp(a, b, t0), p1 = _lerp(a, b, t1)
                let mid = CGPoint(x: (p0.x + p1.x) * 0.5, y: (p0.y + p1.y) * 0.5)
                let inside = _polysContain(clipPolys, mid, eoFill: eoFill)
                if inside == keepInside {
                    if let le = lastEnd, _dist(le, p0) < 1e-5 {
                        out.addLine(to: p1)
                    } else {
                        out.move(to: p0)
                        out.addLine(to: p1)
                    }
                    lastEnd = p1
                } else {
                    lastEnd = nil
                }
            }
        }
    }
    return out
}
