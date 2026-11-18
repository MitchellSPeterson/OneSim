import SwiftUI

// Exercises presentation APIs beyond the showcase (item/presenting variants, onDismiss, detents, locked sheets).
struct Pick: Identifiable { let id: Int }

struct Root: View {
    @State private var dismissed = 0
    @State private var plain = false
    @State private var locked = false
    @State private var tall = false
    @State private var item: Pick?
    @State private var coverItem: Pick?
    @State private var alertData: String?
    @State private var alertOn = false
    @State private var dialogOn = false
    @State private var dialogData: Int?

    var body: some View {
        VStack(spacing: 8) {
            Text("Dismissed: \(dismissed)")
            Button("Plain") { plain = true }
            Button("Clear plain") { plain = false }
            Button("Locked") { locked = true }
            Button("Tall") { tall = true }
            Button("Item") { item = Pick(id: 3) }
            Button("Cover item") { coverItem = Pick(id: 5) }
            Button("Alert") { alertData = "Ada"; alertOn = true }
            Button("Dialog") { dialogData = 9; dialogOn = true }
        }
        .sheet(isPresented: $plain, onDismiss: { dismissed += 1 }) { Text("Plain sheet") }
        .sheet(isPresented: $locked) {
            Text("Locked").interactiveDismissDisabled()
        }
        .sheet(isPresented: $tall) {
            Text("Tall").presentationDetents([.fraction(0.4), .height(300), .large])
        }
        .sheet(item: $item) { p in Text("Item \(p.id)") }
        .fullScreenCover(item: $coverItem) { p in Text("Cover \(p.id)") }
        .alert("Hello", isPresented: $alertOn, presenting: alertData) { name in
            Button("Hi \(name)") {}
        } message: { name in
            Text("Greeting \(name)")
        }
        .confirmationDialog("Pick", isPresented: $dialogOn, presenting: dialogData) { n in
            Button("Use \(n)") {}
        }
    }
}

@main
struct PresentationApp: App {
    var body: some Scene { WindowGroup { Root() } }
}
