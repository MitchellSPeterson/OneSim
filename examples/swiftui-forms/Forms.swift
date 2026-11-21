import SwiftUI

// Forms and containers: Form and List styles, Section headers and footers, LabeledContent, DisclosureGroup,
// GroupBox, ControlGroup, ContentUnavailableView, OutlineGroup, swipeActions, EditButton and Table. Builds unchanged
// against Apple's SwiftUI (scripts/build-ios-sim.sh); the reference states are in
// tests/fixtures/ios27/swiftui-forms-ios/ (iOS 27.2) and tests/fixtures/swiftui-forms-ios/ (iOS 26.5).

@main
struct FormsApp: App {
    var body: some Scene {
        WindowGroup { FormsHome() }
    }
}

struct FormsHome: View {
    @State private var advanced = false
    @State private var sync = true

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    LabeledContent("Name", value: "OneSim")
                    LabeledContent("Version", value: "27.2")
                    LabeledContent("Storage") {
                        Text("64 of 128 GB")
                    }
                } header: {
                    Text("About")
                } footer: {
                    Text("Values are LabeledContent rows.")
                }
                Section {
                    DisclosureGroup("Advanced", isExpanded: $advanced) {
                        Toggle("Sync", isOn: $sync)
                        LabeledContent("Build", value: "24B8117")
                    }
                }
                Section("Lists") {
                    NavigationLink("Grouped") { StyledList(style: .grouped, title: "Grouped") }
                    NavigationLink("Plain") { StyledList(style: .plain, title: "Plain") }
                    NavigationLink("Inset") { StyledList(style: .inset, title: "Inset") }
                    NavigationLink("Sidebar") { SidebarScreen() }
                }
                Section("Containers") {
                    NavigationLink("Boxes") { BoxesScreen() }
                    NavigationLink("Columns") { ColumnsScreen() }
                    NavigationLink("Empty") { EmptyScreen() }
                    NavigationLink("Outline") { OutlineScreen() }
                    NavigationLink("Edit") { EditScreen() }
                    NavigationLink("Swipe") { SwipeScreen() }
                    NavigationLink("Table") { TableScreen() }
                }
            }
            .navigationTitle("Forms")
        }
    }
}

struct StyledList<S: ListStyle>: View {
    let style: S
    let title: String

    var body: some View {
        List {
            Section {
                Text("Apple")
                Text("Banana")
                Label("Cherry", systemImage: "leaf")
            } header: {
                Text("Fruit")
            } footer: {
                Text("Three kinds of fruit.")
            }
            Section("Vegetables") {
                Text("Carrot")
                Text("Pea")
            }
        }
        .listStyle(style)
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.inline)
    }
}

struct SidebarScreen: View {
    @State private var fruit = true
    @State private var vegetables = false

    var body: some View {
        List {
            Section("Fruit", isExpanded: $fruit) {
                Label("Apple", systemImage: "apple.logo")
                Label("Cherry", systemImage: "leaf")
            }
            Section("Vegetables", isExpanded: $vegetables) {
                Label("Carrot", systemImage: "carrot")
            }
        }
        .listStyle(.sidebar)
        .navigationTitle("Sidebar")
        .navigationBarTitleDisplayMode(.inline)
    }
}

struct BoxesScreen: View {
    @State private var details = true

    var body: some View {
        ScrollView {
            VStack(spacing: 16) {
                GroupBox("Storage") {
                    LabeledContent("Used", value: "64 GB")
                    LabeledContent("Free", value: "64 GB")
                }
                GroupBox {
                    Text("A box without a label.")
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                DisclosureGroup("Details", isExpanded: $details) {
                    Text("Shown while expanded.")
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                ControlGroup {
                    Button("Cut") {}
                    Button("Copy") {}
                    Button("Paste") {}
                }
                LabeledContent("Status", value: "Ready")
            }
            .padding()
        }
        .navigationTitle("Boxes")
        .navigationBarTitleDisplayMode(.inline)
    }
}

struct ColumnsScreen: View {
    @State private var wifi = true

    var body: some View {
        Form {
            Section("Network") {
                Toggle("Wi-Fi", isOn: $wifi)
                LabeledContent("Name", value: "Home")
            }
            Section("Device") {
                LabeledContent("Model", value: "iPhone")
            }
        }
        .formStyle(.columns)
        .navigationTitle("Columns")
        .navigationBarTitleDisplayMode(.inline)
    }
}

struct EmptyScreen: View {
    var body: some View {
        ContentUnavailableView {
            Label("No Mail", systemImage: "tray")
        } description: {
            Text("New mail you receive appears here.")
        } actions: {
            NavigationLink("Search") { SearchEmptyScreen() }
        }
        .navigationTitle("Empty")
        .navigationBarTitleDisplayMode(.inline)
    }
}

struct SearchEmptyScreen: View {
    var body: some View {
        ContentUnavailableView.search(text: "kiwi")
            .navigationTitle("Search")
            .navigationBarTitleDisplayMode(.inline)
    }
}

struct Item: Identifiable, Hashable {
    let id: String
    var children: [Item]? = nil
}

let tree: [Item] = [
    Item(id: "Documents", children: [Item(id: "Notes.txt"), Item(id: "Photos", children: [Item(id: "Beach.jpg")])]),
    Item(id: "Music", children: [Item(id: "Song.m4a")]),
    Item(id: "Readme.md"),
]

struct OutlineScreen: View {
    var body: some View {
        List(tree, children: \.children) { item in
            Text(item.id)
        }
        .navigationTitle("Outline")
        .navigationBarTitleDisplayMode(.inline)
    }
}

struct EditScreen: View {
    @State private var names = ["Anna", "Ben", "Chloe", "Dev"]

    var body: some View {
        List {
            Section {
                ForEach(names, id: \.self) { Text($0) }
                    .onDelete { names.remove(atOffsets: $0) }
                    .onMove { names.move(fromOffsets: $0, toOffset: $1) }
            }
            Section {
                EditButton()
            }
        }
        .navigationTitle("Edit")
        .navigationBarTitleDisplayMode(.inline)
    }
}

struct SwipeScreen: View {
    @State private var mail = ["Lunch", "Invoice", "Trip"]
    @State private var flagged: Set<String> = []

    var body: some View {
        List {
            ForEach(mail, id: \.self) { m in
                Text(flagged.contains(m) ? "\(m) (flagged)" : m)
                    .swipeActions {
                        Button("Delete", role: .destructive) { mail.removeAll { $0 == m } }
                        Button("Flag") { flagged.insert(m) }
                            .tint(.orange)
                    }
                    .swipeActions(edge: .leading) {
                        Button("Read") {}
                            .tint(.blue)
                    }
            }
        }
        .navigationTitle("Swipe")
        .navigationBarTitleDisplayMode(.inline)
    }
}

struct Person: Identifiable {
    let id: Int
    let name: String
    let email: String
}

let people = [
    Person(id: 1, name: "Anna", email: "anna@example.com"),
    Person(id: 2, name: "Ben", email: "ben@example.com"),
    Person(id: 3, name: "Chloe", email: "chloe@example.com"),
]

struct TableScreen: View {
    var body: some View {
        Table(people) {
            TableColumn("Name", value: \.name)
            TableColumn("Email", value: \.email)
        }
        .navigationTitle("Table")
        .navigationBarTitleDisplayMode(.inline)
    }
}
