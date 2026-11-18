import SwiftUI

// Exercises Group, List(data), @EnvironmentObject, onAppear/onDisappear, value-based navigation
// (NavigationPath and [T] paths), destination links in a path-bound stack, unmatched values, modifiers on a Group, dismiss and dynamicTypeSize. Must compile unchanged against Apple's SwiftUI.

final class Session: ObservableObject {
    @Published var user = "ada"
    @Published var appeared = 0
    @Published var disappeared = 0
}

struct Item: Hashable { let id: Int; let name: String }
struct Person: Identifiable { let id: Int; let name: String }
enum Route: Hashable { case lists }

@main
struct DataflowApp: App {
    let session = Session()
    var body: some Scene {
        WindowGroup { ContentView().environmentObject(session) }
    }
}

struct ContentView: View {
    var body: some View {
        VStack {
            HomeView()
            TypedView()
        }
    }
}

struct HomeView: View {
    @EnvironmentObject var session: Session
    @Environment(\.dynamicTypeSize) var typeSize
    @State private var path = NavigationPath()

    var body: some View {
        NavigationStack(path: $path) {
            List {
                Section("Session") {
                    Text("User: \(session.user)")
                    Text("Appeared: \(session.appeared)")
                    Text("Disappeared: \(session.disappeared)")
                    Text("Path: \(path.count)")
                    Text("Type size: \(String(describing: typeSize))")
                }
                Section("Links") {
                    NavigationLink("Alpha", value: Item(id: 1, name: "Alpha"))
                    NavigationLink(value: 2) { Text("Number 2") }
                    NavigationLink("Lists", value: Route.lists)
                    NavigationLink("Plain") { Text("Plain pushed").navigationTitle("Plain") }
                    NavigationLink("Orphan", value: 3.5)   // no destination for Double: pushes nothing
                    Group {
                        Text("Group A")
                        Text("Group B")
                    }
                    .opacity(0.5)   // applies to each child
                }
            }
            .navigationTitle("Dataflow")
            .onAppear { session.appeared += 1 }
            .onDisappear { session.disappeared += 1 }
            .navigationDestination(for: Item.self) { ItemScreen(item: $0, depth: path.count) }
            .navigationDestination(for: Int.self) { NumberScreen(n: $0, depth: path.count) }
            .navigationDestination(for: Route.self) { _ in ListsScreen() }
        }
    }
}

struct ItemScreen: View {
    let item: Item
    let depth: Int
    @EnvironmentObject var session: Session
    @Environment(\.dismiss) var dismiss

    var body: some View {
        List {
            Text("Item: \(item.name)")
            Text("User: \(session.user)")
            Text("Counts: \(session.appeared)/\(session.disappeared)")
            Text("Depth: \(depth)")
            Button("Rename") { session.user = "grace" }
            Button("Dismiss") { dismiss() }
        }
        .navigationTitle("Item \(item.name)")
    }
}

struct NumberScreen: View {
    let n: Int
    let depth: Int

    var body: some View {
        List {
            Text("Number: \(n)")
            Text("Depth: \(depth)")
            NavigationLink("Nested", value: Item(id: 3, name: "Nested"))
            NavigationLink("Detail") { Text("Detail depth \(depth)").navigationTitle("Detail") }   // on top of the path
        }
        .navigationTitle("Number \(n)")
    }
}

struct ListsScreen: View {
    let people = [Person(id: 1, name: "Ada"), Person(id: 2, name: "Grace")]

    var body: some View {
        VStack {
            List(people) { Text("Person \($0.name)") }
            List(["x", "y"], id: \.self) { Text("Key \($0)") }
            List(0..<3) { Text("Range \($0)") }
        }
        .navigationTitle("Lists")
    }
}

struct TypedView: View {
    @State private var ids: [Int] = []

    var body: some View {
        NavigationStack(path: $ids) {
            List {
                Text("Typed depth: \(ids.count)")
                NavigationLink("Seven", value: 7)
            }
            .navigationTitle("Typed")
            .navigationDestination(for: Int.self) { n in
                Text("Typed \(n) depth \(ids.count)").navigationTitle("Typed \(n)")
            }
        }
    }
}
