import SwiftUI

// Exercises onChange (3 forms), onTapGesture, @AppStorage, Lazy stacks, Group, List(data), @EnvironmentObject, onAppear/onDisappear, value-based navigation
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
            ReactiveView()
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

struct ReactiveView: View {
    @State private var count = 0
    @State private var log: [String] = []
    @State private var tapped = 0
    @AppStorage("dataflow.launches") private var launches = 0
    @AppStorage("dataflow.flag") private var flag = false
    @AppStorage("dataflow.name") private var name = "none"

    var body: some View {
        LazyVStack(alignment: .leading, spacing: 4, pinnedViews: []) {
            Text("Count: \(count)")
            Text("Log: \(log.joined(separator: ","))")
            Text("Mirror: \(count * 10)")
            LazyHStack(spacing: 2) {
                Text("Tapped: \(tapped)")
                Text("Flag: \(flag ? "on" : "off")")
                Text("Name: \(name)")
                Text("Launches: \(launches)")
            }
            Button("Bump") { count += 1 }
            Button("Store") { launches += 1; flag.toggle(); name = "set" }
            Button("Reset store") { launches = 0; flag = false; name = "none" }
            Text("Tap me").onTapGesture { tapped += 1 }
        }
        .onChange(of: count) { old, new in log.append("\(old)>\(new)") }
        .onChange(of: count, initial: true) { log.append("c") }
        .onChange(of: count) { new in if new == 2 { count = 20 } }   // dirties state: re-evaluates and fires the others again
    }
}
