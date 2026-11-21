import SwiftUI

@main
struct NavigationDemo: App {
    var body: some Scene {
        WindowGroup { NavigationDemoView() }
    }
}

struct NavigationDemoView: View {
    @State private var column: NavigationSplitViewColumn = .sidebar

    var body: some View {
        NavigationSplitView(preferredCompactColumn: $column) {
            List {
                Button("Open detail") { column = .detail }
                NavigationLink("Legacy") { LegacyScreen() }
                NavigationLink("Search") { SearchScreen() }
            }
            .navigationTitle("Library")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("About", systemImage: "info.circle") { column = .detail }
                }
            }
        } detail: {
            DetailScreen()
        }
    }
}

private struct DetailScreen: View {
    @State private var marked = false

    var body: some View {
        Text(marked ? "Book marked" : "Choose a book")
            .navigationTitle("Detail")
            .toolbarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .bottomBar) {
                    Button("Mark", systemImage: "bookmark") { marked.toggle() }
                }
            }
            .toolbarTitleMenu {
                Button("Reset mark") { marked = false }
            }
            .toolbarBackground(Color.white, for: .navigationBar, .bottomBar)
            .toolbarBackgroundVisibility(.visible, for: .navigationBar, .bottomBar)
    }
}

private struct LegacyScreen: View {
    var body: some View {
        NavigationView {
            Text("Archive").navigationTitle("Archive")
        }
        .navigationViewStyle(.stack)
    }
}

private struct SearchScreen: View {
    @State private var query = ""
    @State private var presented = false

    var body: some View {
        List {
            SearchStatus(query: query)
        }
        .navigationTitle("Search")
        .navigationBarTitleDisplayMode(.inline)
        .searchable(text: $query, isPresented: $presented, placement: .navigationBarDrawer(displayMode: .always))
    }
}

private struct SearchStatus: View {
    let query: String
    @Environment(\.isSearching) private var searching
    @Environment(\.dismissSearch) private var dismissSearch

    var body: some View {
        Text(query.isEmpty ? "All books" : query)
        if searching {
            Button("Reset search") { dismissSearch() }
        }
    }
}
