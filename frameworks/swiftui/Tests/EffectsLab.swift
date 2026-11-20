import SwiftUI

struct EffectsLab: View {
    @State private var draws = 1

    var body: some View {
        VStack {
            Button("Draw more") { draws = 300 }
            Canvas { context, size in
                let rect = Path(CGRect(origin: .zero, size: size))
                var clipped = context
                clipped.clip(to: rect, options: .inverse)
                clipped.opacity = 0.25
                clipped.translateBy(x: 3, y: -2)
                clipped.fill(rect, with: .color(.red))
                for _ in 0..<draws {
                    context.stroke(rect, with: .foreground, style: StrokeStyle(lineWidth: 3, dash: [1, 2]))
                }
                if let mark = context.resolveSymbol(id: "星") {
                    context.draw(mark, at: CGPoint(x: size.width / 2, y: size.height / 2))
                }
                var masked = context
                masked.clipToLayer { layer in
                    layer.fill(Path(CGRect(x: 0, y: 0, width: size.width / 2, height: size.height)), with: .color(.white))
                }
                masked.fill(Path(masked.clipBoundingRect), with: .color(.green))
                var text = context.resolve(Text("Red").foregroundColor(.red))
                text.shading = .color(.blue)
                context.draw(text, at: .zero)
            } symbols: {
                Text("星").tag("星")
            }
            .frame(width: 44, height: 28)
            Rectangle().fill(AnyShapeStyle(.linearGradient(
                Gradient(colors: [.red, .blue]).colorSpace(.perceptual), startPoint: .leading, endPoint: .trailing)))
                .frame(width: 80, height: 20)
                .visualEffect { effect, geometry in
                    effect.offset(x: geometry.frame(in: .global).minX + geometry.size.width)
                        .scaleEffect(x: -1, y: 0, anchor: .topTrailing)
                        .blendMode(.multiply)
                }
        }
    }
}
