import SwiftUI

struct NavigationLab: View {
    @State private var column: NavigationSplitViewColumn = .content

    var body: some View {
        NavigationSplitView(preferredCompactColumn: $column) {
            Button("Open content") { column = .content }
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
