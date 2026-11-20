import SwiftUI

@main
struct EffectsApp: App {
    var body: some Scene { WindowGroup { EffectsView() } }
}

struct EffectsView: View {
    private let ocean = Color(red: 0.02, green: 0.36, blue: 0.86)
    private let mint = Color(red: 0.28, green: 0.93, blue: 0.69)
    private let violet = Color(red: 0.66, green: 0.30, blue: 0.94)

    var body: some View {
        VStack(spacing: 18) {
            HStack(spacing: 14) {
                RoundedRectangle(cornerRadius: 18)
                    .fill(.linearGradient(colors: [ocean, mint], startPoint: .topLeading, endPoint: .bottomTrailing))
                Circle()
                    .strokeBorder(.conicGradient(colors: [mint, ocean, violet, mint], center: .center), lineWidth: 16)
                RoundedRectangle(cornerRadius: 18)
                    .fill(.ellipticalGradient(colors: [.white, violet, ocean]))
            }
            .frame(height: 100)

            RoundedRectangle(cornerRadius: 20)
                .fill(MeshGradient(width: 2, height: 2, points: [.init(0, 0), .init(1, 0), .init(0, 1), .init(1, 1)],
                                   colors: [ocean, violet, mint, ocean]))
                .overlay {
                    Circle().fill(.radialGradient(colors: [.white, .clear], center: .center, startRadius: 3, endRadius: 40))
                        .frame(width: 80, height: 80)
                }
                .frame(height: 102)

            Canvas { context, size in
                let middle = size.width / 2
                let shape = Path(roundedRect: CGRect(x: 0, y: 0, width: size.width, height: size.height), cornerRadius: 18)
                context.fill(shape, with: .linearGradient(Gradient(colors: [ocean, violet]),
                                                          startPoint: .zero, endPoint: CGPoint(x: size.width, y: 0)))
                var waveform = Path()
                waveform.move(to: CGPoint(x: middle - 44, y: 54))
                waveform.addLine(to: CGPoint(x: middle - 22, y: 54))
                waveform.addLine(to: CGPoint(x: middle - 10, y: 25))
                waveform.addLine(to: CGPoint(x: middle + 10, y: 77))
                waveform.addLine(to: CGPoint(x: middle + 23, y: 54))
                waveform.addLine(to: CGPoint(x: middle + 44, y: 54))
                context.stroke(waveform, with: .color(.white), style: StrokeStyle(lineWidth: 5, lineCap: .round, lineJoin: .round))
            }
            .frame(height: 104)

            HStack(spacing: 28) {
                RoundedRectangle(cornerRadius: 14).fill(ocean)
                    .frame(width: 76, height: 58)
                    .rotationEffect(.degrees(20))
                    .scaleEffect(x: -1, y: 1.15)
                    .blur(radius: 2)
                Rectangle().fill(violet)
                    .frame(width: 76, height: 76)
                    .mask { Circle().fill(.linearGradient(colors: [.white, .clear], startPoint: .leading, endPoint: .trailing)) }
                ZStack {
                    Circle().fill(mint).offset(x: -10)
                    Circle().fill(violet).offset(x: 10).blendMode(.multiply)
                }
                .frame(width: 72, height: 72)
                .compositingGroup()
            }
            .frame(height: 112)

            Capsule().fill(mint)
                .frame(width: 140, height: 24)
                .visualEffect { effect, geometry in
                    effect.offset(x: geometry.size.width / 6).saturation(0.4)
                }
        }
        .padding(24)
    }
}
