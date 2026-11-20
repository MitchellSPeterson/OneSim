import SwiftUI

struct Diamond: Shape {
    func path(in rect: CGRect) -> Path {
        Path { p in
            p.move(to: CGPoint(x: rect.midX, y: rect.minY))
            p.addLine(to: CGPoint(x: rect.maxX, y: rect.midY))
            p.addLine(to: CGPoint(x: rect.midX, y: rect.maxY))
            p.addLine(to: CGPoint(x: rect.minX, y: rect.midY))
            p.closeSubpath()
        }
    }
}

struct FixedBadge: Shape {
    var size = CGSize(width: 48, height: 32)

    func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize { size }
    func path(in rect: CGRect) -> Path { Path(roundedRect: rect, cornerRadius: 8) }
}

struct FittingSquare: Shape {
    func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
        let size = proposal.replacingUnspecifiedDimensions()
        let side = min(size.width, size.height)
        return CGSize(width: side, height: side)
    }

    func path(in rect: CGRect) -> Path { Path(rect) }
}

private func runPathChecks() -> String {
    var failures: [String] = []
    func check(_ ok: Bool, _ msg: String) { if !ok { failures.append(msg) } }

    // 1. Path builders, description & lossless string round-trip
    let p = Path { p in
        p.move(to: CGPoint(x: 0, y: 0))
        p.addLine(to: CGPoint(x: 40, y: 0))
        p.addQuadCurve(to: CGPoint(x: 40, y: 40), control: CGPoint(x: 60, y: 20))
        p.addCurve(to: CGPoint(x: 0, y: 40), control1: CGPoint(x: 30, y: 50), control2: CGPoint(x: 10, y: 50))
        p.closeSubpath()
    }
    check(!p.isEmpty, "non-empty")
    check(p.currentPoint == CGPoint(x: 0, y: 0), "closeSubpath resets currentPoint")
    let desc = p.description
    if let parsed = Path(desc) {
        check(parsed == p, "Path(description) round-trip")
    } else {
        failures.append("Path(description) returned nil for \(desc)")
    }
    check(Path("bad tokens") == nil, "invalid description returns nil")

    // Exact Bezier boundingRect (quad control at x=60 -> max x is 50, not 60!)
    let br = p.boundingRect
    check(abs(br.minX - 0) < 0.01 && abs(br.maxX - 50) < 0.01, "quad extrema maxX=\(br.maxX)")

    // 2. contains with winding vs eoFill
    let nested = Path { p in
        p.addRect(CGRect(x: 0, y: 0, width: 40, height: 40))
        p.addRect(CGRect(x: 10, y: 10, width: 20, height: 20))
    }
    check(nested.contains(CGPoint(x: 5, y: 5), eoFill: false), "outer winding")
    check(nested.contains(CGPoint(x: 20, y: 20), eoFill: false), "center winding")
    check(!nested.contains(CGPoint(x: 20, y: 20), eoFill: true), "center eoFill hole")

    // 3. trimmedPath
    let line = Path { p in p.move(to: CGPoint(x: 0, y: 0)); p.addLine(to: CGPoint(x: 100, y: 0)) }
    let half = line.trimmedPath(from: 0.25, to: 0.75)
    check(abs(half.boundingRect.minX - 25) < 0.1 && abs(half.boundingRect.maxX - 75) < 0.1, "trimmed line")
    check(line.trimmedPath(from: 0.6, to: 0.2).isEmpty, "reversed trim empty")

    // 4. strokedPath
    let stroked = line.strokedPath(StrokeStyle(lineWidth: 10, lineCap: .butt))
    check(stroked.contains(CGPoint(x: 50, y: 4)), "stroked interior")
    check(!stroked.contains(CGPoint(x: 50, y: 8)), "stroked outside")

    let collinearCubic = Path { p in
        p.move(to: .zero)
        let control = CGPoint(x: 100, y: 0)
        p.addCurve(to: CGPoint(x: 10, y: 0), control1: control, control2: control)
    }
    let cubicStroke = collinearCubic.strokedPath(StrokeStyle(lineWidth: 10))
    check(cubicStroke.contains(CGPoint(x: 50, y: 0)), "collinear cubic stroke preserves overshoot")

    // 5. Boolean operations on overlapping rects [0..40, 0..40] and [20..60, 0..40]
    let a = Path(CGRect(x: 0, y: 0, width: 40, height: 40))
    let b = Path(CGRect(x: 20, y: 0, width: 40, height: 40))
    let u = a.union(b)
    check(u.contains(CGPoint(x: 10, y: 20)) && u.contains(CGPoint(x: 30, y: 20)) && u.contains(CGPoint(x: 50, y: 20)), "union")
    let inter = a.intersection(b)
    check(!inter.contains(CGPoint(x: 10, y: 20)) && inter.contains(CGPoint(x: 30, y: 20)) && !inter.contains(CGPoint(x: 50, y: 20)), "intersection")
    let sub = a.subtracting(b)
    check(sub.contains(CGPoint(x: 10, y: 20)) && !sub.contains(CGPoint(x: 30, y: 20)) && !sub.contains(CGPoint(x: 50, y: 20)), "subtracting")
    let sym = a.symmetricDifference(b)
    check(sym.contains(CGPoint(x: 10, y: 20)) && !sym.contains(CGPoint(x: 30, y: 20)) && sym.contains(CGPoint(x: 50, y: 20)), "symmetricDifference")

    // 6. Hole subtraction (rect minus inner rect)
    let hole = a.subtracting(Path(CGRect(x: 10, y: 10, width: 20, height: 20)))
    check(hole.contains(CGPoint(x: 5, y: 5)) && !hole.contains(CGPoint(x: 20, y: 20)), "hole subtraction")

    // 7. lineIntersection & lineSubtraction
    let diag = Path { p in p.move(to: CGPoint(x: -10, y: 20)); p.addLine(to: CGPoint(x: 50, y: 20)) }
    let li = diag.lineIntersection(a)
    check(abs(li.boundingRect.minX - 0) < 0.2 && abs(li.boundingRect.maxX - 40) < 0.2, "lineIntersection \(li.boundingRect)")
    let ls = diag.lineSubtraction(a)
    var lsCount = 0
    ls.forEach { if case .move = $0 { lsCount += 1 } }
    check(lsCount == 2, "lineSubtraction splits into 2 pieces: \(lsCount)")

    // 8. Shape transforms & InsettableShape
    let r = CGRect(x: 0, y: 0, width: 100, height: 60)
    let scaled = Rectangle().scale(0.5, anchor: .center).path(in: r)
    check(abs(scaled.boundingRect.minX - 25) < 0.1 && abs(scaled.boundingRect.width - 50) < 0.1, "ScaledShape")
    let moved = Rectangle().offset(x: 10, y: 20).path(in: r)
    check(abs(moved.boundingRect.minX - 10) < 0.1 && abs(moved.boundingRect.minY - 20) < 0.1, "OffsetShape")
    let rot = Rectangle().rotation(.degrees(90)).path(in: CGRect(x: 0, y: 0, width: 100, height: 100))
    check(abs(rot.boundingRect.width - 100) < 0.2 && abs(rot.boundingRect.height - 100) < 0.2, "RotatedShape")
    let sized = Rectangle().size(width: 40, height: 20, anchor: .center).path(in: r)
    check(abs(sized.boundingRect.minX - 30) < 0.1 && abs(sized.boundingRect.minY - 20) < 0.1, "SizedShape")
    let insetRect = RoundedRectangle(cornerRadius: 20).inset(by: 5).path(in: r)
    check(abs(insetRect.boundingRect.minX - 5) < 0.1 && abs(insetRect.boundingRect.width - 90) < 0.1, "InsettableShape")
    let erased = AnyShape(Circle())
    check(erased.sizeThatFits(ProposedViewSize(width: 100, height: 60)) == CGSize(width: 60, height: 60), "AnyShape preserves Circle sizeThatFits")
    check(Rectangle().layoutDirectionBehavior == .fixed && Diamond().layoutDirectionBehavior == .mirrors, "layoutDirectionBehavior")

    #if canImport(CoreGraphics)
    let cg = p.cgPath
    let fromCG = Path(cg)
    check(fromCG == p, "CGPath round-trip")
    #endif

    return failures.isEmpty ? "OK" : "FAIL: " + failures.joined(separator: "; ")
}

public struct ShapeLab: View {
    public init() {}
    public var body: some View {
        VStack(spacing: 8) {
            Text(runPathChecks())
            Ellipse().fill(.blue)
            UnevenRoundedRectangle(topLeadingRadius: 20, bottomLeadingRadius: 4, bottomTrailingRadius: 16, topTrailingRadius: 8)
                .fill(.green, style: FillStyle(eoFill: true, antialiased: false))
                .strokeBorder(.black, style: StrokeStyle(lineWidth: 4, lineCap: .round, lineJoin: .bevel, dash: [8, 4], dashPhase: 2))
            VStack {
                ContainerRelativeShape().fill(.orange)
                ConcentricRectangle(topLeadingCorner: .concentric, topTrailingCorner: .concentric(minimum: 6), bottomLeadingCorner: .fixed(4), bottomTrailingCorner: .concentric)
                    .fill(.purple)
            }
            .padding(10)
            .containerShape(RoundedRectangle(cornerRadius: 28))
            Diamond().fill(.red)
            FixedBadge().stroke(.blue, lineWidth: 2)
            AnyShape(Circle()).fill(.yellow)
            Circle().trim(from: 0, to: 0.75).rotation(.degrees(-90)).stroke(.pink, lineWidth: 5)
            Rectangle().subtracting(Circle().scale(0.5)).fill(.gray)
            Group {
                Circle().strokeBorder(lineWidth: 4).foregroundStyle(.red)
                Rectangle().inset(by: 4).strokeBorder(.blue, lineWidth: 6)
                Circle().fill(.green).stroke(lineWidth: 2)
                FixedBadge(size: CGSize(width: 100, height: 100)).fill(.blue)
                FittingSquare().fill(.blue)
            }
        }
    }
}
