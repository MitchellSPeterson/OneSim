import SwiftUI

// Wire-test app (#49): a data-presenting WindowGroup is the window, its value binding keeps scene state, and an
// `if #available` scene builds.

struct Item: Codable, Hashable { var n: Int }

@main
struct SceneDataLab: App {
    @State var launches = 0

    var body: some Scene {
        WindowGroup("Items", for: Item.self) { $item in
            VStack {
                Text("Item \(item.n)")
                Button("Next") { item.n += 1 }
                Button("App") { launches += 1 }
                Text("App \(launches)")
            }
        } defaultValue: {
            Item(n: 5)
        }
        if #available(iOS 17, macOS 14, *) {
            WindowGroup { Text("Never shown") }
        }
    }
}
