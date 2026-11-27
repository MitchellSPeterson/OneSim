import SwiftUI

@main
struct BlurReplaceApp: App {
    var body: some Scene {
        WindowGroup { BlurReplaceDemo() }
    }
}

struct BlurReplaceDemo: View {
    @State private var replaced = false

    var body: some View {
        VStack(spacing: 28) {
            HStack(spacing: 24) {
                card(configuration: .downUp)
                card(configuration: .upUp)
            }
            Button("Replace cards") {
                // Slow enough to capture both directions in the same frame.
                withAnimation(.linear(duration: 12)) { replaced.toggle() }
            }
        }
    }

    private func card(configuration: BlurReplaceTransition.Configuration) -> some View {
        ZStack {
            if replaced {
                tile("New", color: .orange).transition(.blurReplace(configuration))
            } else {
                tile("Old", color: .blue).transition(.blurReplace(configuration))
            }
        }
        .frame(width: 120, height: 48)
    }

    private func tile(_ label: String, color: Color) -> some View {
        RoundedRectangle(cornerRadius: 9)
            .fill(color)
            .frame(width: 120, height: 48)
            .overlay { Text(label).foregroundStyle(Color.white) }
    }
}
