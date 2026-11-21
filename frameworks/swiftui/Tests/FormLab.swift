import SwiftUI

// Forms and containers behaviour (wire_test.py): list styles, sections, edit mode, swipe actions, disclosure groups,
// outlines, custom container styles, columns forms, tables and empty states.

struct FormLab: View {
    var body: some View {
        NavigationStack {
            List {
                NavigationLink("Styles") { StylesLab() }
                NavigationLink("Sections") { SectionsLab() }
                NavigationLink("Editing") { EditingLab() }
                NavigationLink("Swipes") { SwipesLab() }
                NavigationLink("Disclosure") { DisclosureLab() }
                NavigationLink("Custom") { CustomStylesLab() }
                NavigationLink("Misc") { MiscLab() }
                NavigationLink("Override") { OverrideLab() }
                NavigationLink("Disabled") { DisabledLab() }
            }
            .navigationTitle("Lab")
        }
    }
}

struct StylesLab: View {
    var body: some View {
        VStack {
            List { Text("auto") }
            List { Text("grouped") }.listStyle(.grouped)
            List { Text("plain") }.listStyle(.plain)
            List { Text("inset") }.listStyle(.inset)
            List { Text("sidebar") }.listStyle(.sidebar)
            List { Text("insetGrouped") }.listStyle(InsetGroupedListStyle())
            Form { Text("form") }
            Form { Text("formGrouped") }.formStyle(.grouped)
        }
    }
}

struct SectionsLab: View {
    @State private var open = true
    var body: some View {
        List {
            Section {
                Text("one")
            } header: {
                HStack { Image(systemName: "star"); Text("Starred") }
            } footer: {
                Text("Plain footer")
            }
            Section("Folding", isExpanded: $open) {
                Text("hidden when folded")
            }
            Text("loose row")
        }
        .listStyle(.sidebar)
    }
}

struct EditingLab: View {
    @State private var names = ["A", "B", "C", "D"]
    @State private var log = ""
    var body: some View {
        List {
            Section {
                ForEach(names, id: \.self) { Text($0) }
                    .onDelete { names.remove(atOffsets: $0); log += "del " }
                    .onMove { names.move(fromOffsets: $0, toOffset: $1); log += "move " }
            }
            Section {
                EditButton()
                Text("order \(names.joined())")
                Text("log \(log)")
                ForEach(["x"], id: \.self) { Text("static \($0)") }
            }
        }
    }
}

struct SwipesLab: View {
    @State private var log = ""
    @State private var items = ["Mail 1", "Mail 2"]
    var body: some View {
        List {
            Text("log \(log)")
            ForEach(items, id: \.self) { m in
                Text(m)
                    .swipeActions {
                        Button("Delete", role: .destructive) { log += "delete(\(m)) " }
                        Button("Flag") { log += "flag(\(m)) " }.tint(.orange)
                    }
                    .swipeActions(edge: .leading, allowsFullSwipe: false) {
                        Button("Read") { log += "read(\(m)) " }
                    }
            }
            ForEach(["Only delete"], id: \.self) { Text($0) }
                .onDelete { _ in log += "ondelete " }
        }
    }
}

struct Node2: Identifiable {
    let id: String
    var kids: [Node2]? = nil
}

struct DisclosureLab: View {
    @State private var bound = false
    let tree = [Node2(id: "root", kids: [Node2(id: "leaf"), Node2(id: "branch", kids: [Node2(id: "deep")])]), Node2(id: "solo")]
    var body: some View {
        VStack {
            List {
                DisclosureGroup("Internal") { Text("internal child") }
                DisclosureGroup("Bound", isExpanded: $bound) {
                    Text("bound child 1")
                    Text("bound child 2")
                }
            }
            DisclosureGroup("Outside") { Text("outside child") }
            List(tree, children: \.kids) { Text($0.id) }
            Text(bound ? "bound open" : "bound closed")
        }
    }
}

struct BoxedGroup: GroupBoxStyle {
    func makeBody(configuration: Configuration) -> some View {
        VStack { Text("custom box"); configuration.label; configuration.content }
    }
}
struct Arrowed: DisclosureGroupStyle {
    func makeBody(configuration: Configuration) -> some View {
        VStack {
            Button(configuration.isExpanded ? "collapse" : "expand") { configuration.isExpanded.toggle() }
            if configuration.isExpanded { configuration.content }
        }
    }
}
struct Stacked: LabeledContentStyle {
    func makeBody(configuration: Configuration) -> some View {
        VStack { configuration.label; configuration.content; Text("stacked") }
    }
}
struct Bracketed: ControlGroupStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack { Text("["); configuration.content; Text("]") }
    }
}
struct Framed: FormStyle {
    func makeBody(configuration: Configuration) -> some View {
        VStack { Text("framed form"); Form(configuration) }
    }
}
struct DefaultLook: LabeledContentStyle {
    func makeBody(configuration: Configuration) -> some View { LabeledContent(configuration) }
}

struct CustomStylesLab: View {
    var body: some View {
        ScrollView {
            VStack {
                GroupBox("Box label") { Text("box content") }.groupBoxStyle(BoxedGroup())
                DisclosureGroup("styled") { Text("styled child") }.disclosureGroupStyle(Arrowed())
                LabeledContent("Key", value: "Value").labeledContentStyle(Stacked())
                LabeledContent("Wrapped", value: "Default").labeledContentStyle(DefaultLook())
                ControlGroup { Button("One") {}; Button("Two") {} }.controlGroupStyle(Bracketed())
                ControlGroup("Actions") { Button("Menu item") {} }.controlGroupStyle(.menu)
                Form { Text("in framed") }.formStyle(Framed())
            }
        }
    }
}

struct Row: Identifiable {
    let id: Int
    let name: String
    let note: String
}

struct MiscLab: View {
    @State private var wifi = true
    var body: some View {
        VStack {
            Form {
                Section("Net") {
                    Toggle("Wi-Fi", isOn: $wifi)
                    LabeledContent("Count", value: 1234.5, format: .number)
                }
            }
            .formStyle(.columns)
            Table([Row(id: 1, name: "Ann", note: "x"), Row(id: 2, name: "Bo", note: "y")]) {
                TableColumn("Name", value: \.name)
                TableColumn("Note") { Text($0.note) }
            }
            ContentUnavailableView.search(text: "  ")
            ContentUnavailableView("Gone", systemImage: "xmark", description: Text("Nothing here"))
            GroupBox { GroupBox("Inner") { Text("nested") } }
        }
    }
}

/// `\.editMode` set around a NavigationStack reaches its screens.
struct OverrideLab: View {
    var body: some View {
        NavigationStack {
            List {
                ForEach(["p", "q"], id: \.self) { Text($0) }
                    .onDelete { _ in }
            }
        }
        .environment(\.editMode, .constant(.active))
    }
}

/// A disabled list offers no delete control, Delete action, reorder or swipe, in edit mode or not.
struct DisabledLab: View {
    var body: some View {
        VStack {
            List {
                ForEach(["e1", "e2"], id: \.self) { Text($0) }
                    .onDelete { _ in }
                    .onMove { _, _ in }
            }
            .disabled(true)
            .environment(\.editMode, .constant(.active))
            List {
                ForEach(["s1"], id: \.self) { m in
                    Text(m).swipeActions { Button("Act") {} }
                }
                .onDelete { _ in }
                ForEach(["s2"], id: \.self) { Text($0) }
                    .onDelete { _ in }
            }
            .disabled(true)
        }
    }
}
