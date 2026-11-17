import SwiftUI

struct Badge: View {
    let title: String
    let color: Color
    @State private var taps = 0

    var body: some View {
        Button(action: { taps += 1 }) {
            Text("\(title) \(taps)")
                .padding(.horizontal, 12)
                .padding(8)
                .background(color)
                .cornerRadius(8)
        }
    }
}

struct ContentView: View {
    @State private var on = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("SimCore").foregroundColor(.blue)
                Spacer(minLength: 8)
                Color.orange.frame(width: 24, height: 24).cornerRadius(12)
            }
            if on {
                Text("On").padding().background(Color.green)
            } else {
                Text("Off").padding().background(Color.gray.opacity(0.3))
            }
            ZStack(alignment: .topTrailing) {
                Rectangle().foregroundColor(.purple)
                Text("Z").padding(4)
            }
            .frame(minWidth: 0, maxWidth: .infinity, minHeight: 40, maxHeight: 40)
            Badge(title: "Badge", color: .yellow)
            Button("Toggle") { on.toggle(); print("toggled \(on)") }
        }
        .padding()
    }
}

@main
struct DemoApp: App {
    var body: some Scene {
        WindowGroup { ContentView() }
    }
}
