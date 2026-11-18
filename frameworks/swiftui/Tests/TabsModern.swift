import SwiftUI

// Wire-test fixture: the iOS 18 `Tab` form (examples/swiftui-tabs uses `.tabItem`) with an enum selection and
// a string badge.
struct TabsModern: View {
    enum Pane: Hashable { case one, two }
    @State private var pane = Pane.two
    var body: some View {
        TabView(selection: $pane) {
            Tab("One", systemImage: "1.circle", value: Pane.one) { Note(name: "one") }
            Tab("Two", systemImage: "2.circle", value: Pane.two) { Note(name: "two") }.badge("New")
        }
    }
}

struct Note: View {
    let name: String
    @State private var taps = 0
    var body: some View { Button("\(name) \(taps)") { taps += 1 } }
}
