import SwiftUI

struct NavigationLab: View {
    @State private var column: NavigationSplitViewColumn = .content
    @State private var showToolbars = false
    @State private var showDestinations = false

    var body: some View {
        if showDestinations {
            DestinationProbe()
        } else if showToolbars {
            ToolbarProbe()
        } else {
            split
        }
    }

    private var split: some View {
        NavigationSplitView(preferredCompactColumn: $column) {
            VStack {
                Button("Open content") { column = .content }
                Button("Test toolbars") { showToolbars = true }
                Button("Test destinations") { showDestinations = true }
            }
            .navigationTitle("Sidebar")
        } content: {
            SearchProbe { column = .detail }
        } detail: {
            Text("Detail content").navigationTitle("Detail")
        }
    }
}

private struct DestinationProbe: View {
    @State private var path = [Int.min, Int.max]
    @State private var presented = false
    @State private var booleanTitle = "Boolean destination"
    @State private var item: String?
    @State private var appeared = 0
    @State private var showMixedPath = false

    var body: some View {
        if showMixedPath {
            MixedPathProbe()
        } else {
            typedPath
        }
    }

    private var typedPath: some View {
        NavigationStack(path: $path) {
            VStack {
                Text("Root appeared: \(appeared)")
                Text("Presented: \(presented)")
                Text("Item: \(item ?? "nil")")
                Button("Show Boolean") { presented = true }
                Button("Show item") { item = "書📚 مرحبا" }
                Button("Restore path") { path = [0, -1] }
                Button("Test mixed path") { showMixedPath = true }
            }
            .navigationTitle("Destination root")
            .onAppear { appeared += 1 }
            .navigationDestination(for: Int.self) { value in
                VStack {
                    Text("Path count: \(path.count), root appeared: \(appeared)")
                    Button("Show Boolean above path") { presented = true }
                }
                .navigationTitle("Integer \(value)")
            }
            .navigationDestination(isPresented: $presented) {
                VStack {
                    DestinationDismissButton()
                    Button("Rename Boolean") { booleanTitle = "Renamed Boolean" }
                    Button("Clear Boolean") { presented = false }
                    NavigationLink("Push plain child") {
                        Button("Clear Boolean from child") { presented = false }
                            .navigationTitle("Plain child")
                    }
                }
                .navigationTitle(booleanTitle)
            }
            .navigationDestination(item: $item) { value in
                ItemDestinationProbe(value: value) { item = "" } clear: { item = nil }
            }
        }
    }
}

private struct MixedPathProbe: View {
    @State private var path: NavigationPath
    @State private var presented = true

    init() {
        var path = NavigationPath()
        path.append(17)
        path.append("Restored string")
        _path = State(initialValue: path)
    }

    var body: some View {
        NavigationStack(path: $path) {
            Text("Mixed count: \(path.count)")
                .navigationTitle("Mixed root")
                .navigationDestination(for: Int.self) { number in
                    NavigationLink("Push another string", value: "Another string")
                        .navigationTitle("Integer \(number)")
                        .navigationDestination(for: String.self) { string in
                            Button("Clear path") { path.removeLast(path.count) }
                                .navigationTitle(string)
                        }
                }
                .navigationDestination(isPresented: $presented) {
                    Text("Initial Boolean").navigationTitle("Initial Boolean")
                }
        }
    }
}

private struct ItemDestinationProbe: View {
    let value: String
    let replace: () -> Void
    let clear: () -> Void
    @State private var nested = false
    @State private var count = 0
    @State private var appeared = false

    var body: some View {
        VStack {
            Text("Item value: [\(value)]")
            Text("Item appeared: \(appeared)")
            Button("Count: \(count)") { count += 1 }
            Button("Replace item", action: replace)
            Button("Clear item", action: clear)
            Button("Show nested Boolean") { nested = true }
            DestinationDismissButton()
        }
        .navigationTitle("Item destination")
        .onAppear { appeared = true }
        .navigationDestination(isPresented: $nested) {
            VStack {
                Text("Nested Boolean")
                DestinationDismissButton()
                Button("Clear parent item", action: clear)
                Button("Replace parent item", action: replace)
            }
            .navigationTitle("Nested destination")
        }
    }
}

private struct DestinationDismissButton: View {
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        Button("Dismiss destination") { dismiss() }
    }
}

private struct SearchProbe: View {
    let openDetail: () -> Void
    @State private var query = "Prefilled"
    @State private var presented = false
    @State private var submitted = ""
    @State private var dismissCount = 0

    var body: some View {
        VStack {
            Text(query)
            Text("Submitted: \(submitted)")
            Button("Present search") { presented = true }
            Button("Open detail", action: openDetail)
            DismissButton { dismissCount += 1 }
            Text("Dismissed: \(dismissCount)")
        }
        .navigationTitle("Content")
        .searchable(text: $query, isPresented: $presented, prompt: "Find a title")
        .onSubmit(of: .search) { submitted = query }
    }
}

private struct DismissButton: View {
    @Environment(\.dismissSearch) private var dismissSearch
    let recordAttempt: () -> Void

    var body: some View {
        Button("Dismiss search") {
            dismissSearch()
            recordAttempt()
        }
    }
}


private struct ToolbarProbe: View {
    var body: some View {
        NavigationStack {
            Text("Outer content").navigationTitle("Outer")
        }
        .toolbar {
            CounterToolbar()
            ToolbarItem {
                NavigationLink("Open inner") { InnerToolbarProbe() }
            }
        }
    }
}

private struct CounterToolbar: ToolbarContent {
    @State private var count = 0

    var body: some ToolbarContent {
        ToolbarItem {
            Button("Toolbar count: \(count)") { count += 1 }
        }
    }
}

private struct InnerToolbarProbe: View {
    @State private var path: [Int] = []

    var body: some View {
        NavigationStack(path: $path) {
            Text("Inner content")
                .navigationTitle("Inner")
                .navigationDestination(for: Int.self) { value in
                    Text("Inner destination \(value)").navigationTitle("Inner detail")
                }
        }
        .toolbar {
            ToolbarItem {
                NavigationLink("Open inner detail", value: 42)
            }
        }
    }
}
