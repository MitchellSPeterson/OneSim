import SwiftUI

// Wire-test fixture: the iOS 18 `Tab` form (examples/swiftui-tabs uses `.tabItem`) with an enum selection and
// a text badge and a zero count.
struct TabsModern: View {
    enum Pane: Hashable { case one, two, lifecycle }
    @State private var pane = Pane.two
    var body: some View {
        TabView(selection: $pane) {
            Tab("One", systemImage: "1.circle", value: Pane.one) { Note(name: "one") }.badge(0)
            Tab("Two", systemImage: "2.circle", value: Pane.two) { Note(name: "two") }.badge(Text("New"))
            Tab("Lifecycle", systemImage: "book", value: Pane.lifecycle) { PageLifecycle() }
        }
        .tabViewStyle(.tabBarOnly)
    }
}

struct PageLifecycle: View {
    @Environment(\.isTabViewSidebarAvailable) private var sidebarAvailable
    @State private var selection = 0
    @State private var visits = 0

    var body: some View {
        VStack {
            Text("visits \(visits), sidebar \(sidebarAvailable)")
            TabView(selection: $selection) {
                Text("First").tag(0)
                Text("Second")
                    .onAppear { visits += 1 }
                    .alert("Second page alert", isPresented: .constant(true)) {}
                    .tag(1)
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
        }
    }
}

struct Note: View {
    let name: String
    @State private var taps = 0
    var body: some View { Button("\(name) \(taps)") { taps += 1 } }
}
