import SwiftUI

// Popovers, context menus, presentation styling, ShareLink and PasteButton (issue #45).
// The reference states are listed in tests/fixtures/ios27/swiftui-popovers-ios/README.md.
struct Root: View {
    @State private var adapted = false
    @State private var bubble = false
    @State private var styled = false
    @State private var pasted = "Nothing pasted"
    @State private var favorite = false

    var body: some View {
        VStack(spacing: 20) {
            Button("Popover as sheet") { adapted = true }
                .popover(isPresented: $adapted) {
                    VStack(spacing: 12) {
                        Text("Popover content")
                        Button("Done") { adapted = false }
                    }
                }
            Button("Popover bubble") { bubble = true }
                .popover(isPresented: $bubble, arrowEdge: .top) {
                    Text("Bubble content")
                        .padding()
                        .presentationCompactAdaptation(.popover)
                }
            Button("Styled sheet") { styled = true }
                .sheet(isPresented: $styled) {
                    Text("Styled sheet")
                        .presentationDetents([.medium])
                        .presentationBackground(.yellow)
                        .presentationCornerRadius(16)
                        .presentationBackgroundInteraction(.enabled)
                }
            Text(favorite ? "Favorite" : "Hold for menu")
                .padding()
                .background(.blue.opacity(0.15), in: RoundedRectangle(cornerRadius: 12))
                .contextMenu {
                    Button("Copy", systemImage: "doc.on.doc") {}
                    Button("Favorite", systemImage: "star") { favorite.toggle() }
                    Divider()
                    Button("Delete", systemImage: "trash", role: .destructive) {}
                }
            Text("Hold for preview")
                .contextMenu {
                    Button("Open") {}
                } preview: {
                    Text("Preview")
                        .font(.largeTitle)
                        .padding(40)
                }
            ShareLink(item: "Hello from OneSim")
            ShareLink("Share link", item: URL(string: "https://example.com")!)
            PasteButton(payloadType: String.self) { strings in
                pasted = strings.first ?? ""
            }
            Text(pasted)
        }
    }
}

@main
struct PopoversApp: App {
    var body: some Scene { WindowGroup { Root() } }
}
