import SwiftUI

struct NavigationLab: View {
    @State private var column: NavigationSplitViewColumn = .content
    @State private var showToolbars = false

    var body: some View {
        if showToolbars {
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
            }
            .navigationTitle("Sidebar")
        } content: {
            SearchProbe { column = .detail }
        } detail: {
            Text("Detail content").navigationTitle("Detail")
        }
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
