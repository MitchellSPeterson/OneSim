import SwiftUI

// Shapes and paths: Ellipse, UnevenRoundedRectangle, ContainerRelativeShape, ConcentricRectangle,
// custom Shape, AnyShape, Path with FillStyle (eoFill), StrokeStyle (dash/cap/join),
// InsettableShape.strokeBorder, and shape operations (trim, rotation, scale, offset, union,
// subtracting, intersection).
// Builds unchanged against Apple's SwiftUI (scripts/build-ios-sim.sh); the reference states are in
// tests/fixtures/swiftui-shapes-ios/ (iOS 26.5) and tests/fixtures/ios27/swiftui-shapes-ios/ (iOS 27.2).

@main
struct ShapesApp: App {
    var body: some Scene {
        WindowGroup { ShapesView() }
    }
}

struct StarShape: Shape {
    var points: Int = 5

    func path(in rect: CGRect) -> Path {
        let cx = rect.midX, cy = rect.midY
        let outer = min(rect.width, rect.height) * 0.5
        let inner = outer * 0.42
        let count = max(3, points)
        var p = Path()
        for i in 0..<(count * 2) {
            let angle = (Double(i) * .pi / Double(count)) - (.pi / 2)
            let r = i % 2 == 0 ? outer : inner
            let pt = CGPoint(x: cx + CGFloat(cos(angle)) * r, y: cy + CGFloat(sin(angle)) * r)
            if i == 0 { p.move(to: pt) } else { p.addLine(to: pt) }
        }
        p.closeSubpath()
        return p
    }
}

struct ShapesView: View {
    var body: some View {
        NavigationStack {
            VStack(spacing: 22) {
                HStack(spacing: 16) {
                    Ellipse()
                        .fill(Color.blue)
                        .frame(width: 96, height: 52)
                    UnevenRoundedRectangle(
                        topLeadingRadius: 24,
                        bottomLeadingRadius: 4,
                        bottomTrailingRadius: 24,
                        topTrailingRadius: 4
                    )
                    .fill(Color.green)
                    .frame(width: 96, height: 52)
                    RoundedRectangle(cornerRadius: 14)
                        .fill(Color.orange.opacity(0.2))
                        .strokeBorder(Color.orange, lineWidth: 5)
                        .frame(width: 96, height: 52)
                }

                HStack(spacing: 16) {
                    ContainerRelativeShape()
                        .fill(Color.blue)
                        .padding(10)
                        .frame(width: 152, height: 72)
                        .background(RoundedRectangle(cornerRadius: 26).fill(Color.blue.opacity(0.2)))
                        .containerShape(RoundedRectangle(cornerRadius: 26))

                    ConcentricRectangle(corners: .concentric(minimum: .fixed(6)), isUniform: true)
                        .fill(Color.purple)
                        .padding(10)
                        .frame(width: 152, height: 72)
                        .background(RoundedRectangle(cornerRadius: 26).fill(Color.purple.opacity(0.2)))
                        .containerShape(RoundedRectangle(cornerRadius: 26))
                }

                HStack(spacing: 24) {
                    StarShape(points: 5)
                        .fill(Color.orange)
                        .frame(width: 72, height: 72)

                    AnyShape(StarShape(points: 5).rotation(.degrees(36)).scale(0.88))
                        .stroke(
                            Color.pink,
                            style: StrokeStyle(lineWidth: 3, lineCap: .round, lineJoin: .round, dash: [6, 4])
                        )
                        .frame(width: 72, height: 72)

                    Path { p in
                        p.addRect(CGRect(x: 4, y: 4, width: 64, height: 64))
                        p.addRect(CGRect(x: 20, y: 20, width: 32, height: 32))
                    }
                    .fill(Color.blue, style: FillStyle(eoFill: true))
                    .frame(width: 72, height: 72)
                }

                HStack(spacing: 20) {
                    Circle()
                        .trim(from: 0, to: 0.72)
                        .rotation(.degrees(-90))
                        .stroke(Color.green, style: StrokeStyle(lineWidth: 8, lineCap: .round))
                        .frame(width: 60, height: 60)

                    Circle()
                        .offset(x: -10)
                        .union(Circle().offset(x: 10))
                        .fill(Color.purple)
                        .frame(width: 56, height: 56)

                    RoundedRectangle(cornerRadius: 12)
                        .subtracting(Circle().scale(0.55))
                        .fill(Color.red)
                        .frame(width: 60, height: 60)

                    Circle()
                        .offset(x: -10)
                        .intersection(Circle().offset(x: 10))
                        .fill(Color.orange)
                        .frame(width: 56, height: 56)
                }
            }
            .navigationTitle("Shapes")
        }
    }
}
