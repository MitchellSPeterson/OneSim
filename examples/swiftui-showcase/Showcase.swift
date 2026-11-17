import SwiftUI

// A multi-screen app exercising a broad SwiftUI surface. It must compile
// unchanged against Apple's SwiftUI (iOS 26+) and SimCore's SwiftUI module.

struct TodoItem: Identifiable {
    let id: Int
    var title: String
    var done: Bool
}

final class TodoStore: ObservableObject {
    @Published var items: [TodoItem] = [
        TodoItem(id: 0, title: "Write the PRD", done: true),
        TodoItem(id: 1, title: "Build the renderer", done: true),
        TodoItem(id: 2, title: "Ship the SwiftUI subset", done: false),
    ]

    var openCount: Int { items.filter { !$0.done }.count }

    func add(_ title: String) {
        items.append(TodoItem(id: items.count, title: title, done: false))
    }
}

@main
struct ShowcaseApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}

struct ContentView: View {
    @StateObject private var store = TodoStore()

    var body: some View {
        NavigationStack {
            List {
                Section("Screens") {
                    NavigationLink("Typography") { TypographyScreen() }
                    NavigationLink("Shapes") { ShapesScreen() }
                    NavigationLink("Todos") { TodoScreen(store: store) }
                    NavigationLink("Controls") { ControlsScreen() }
                    NavigationLink("Symbols") { SymbolsScreen() }
                    NavigationLink("Glass") { GlassScreen() }
                }
                Section("Summary") {
                    HStack {
                        Text("Open todos")
                        Spacer()
                        Text("\(store.openCount)")
                            .foregroundStyle(.secondary)
                    }
                }
            }
            .navigationTitle("Showcase")
        }
    }
}

struct TypographyScreen: View {
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Text("Large Title").font(.largeTitle)
                Text("Title").font(.title)
                Text("Headline").font(.headline)
                Text("Body text that is long enough to wrap onto a second line on a typical phone.")
                    .font(.body)
                Text("Caption, secondary").font(.caption).foregroundStyle(.secondary)
                Text("Bold body").bold()
                Text("System 22 semibold").font(.system(size: 22, weight: .semibold))
                Text(colorScheme == .dark ? "Dark appearance" : "Light appearance")
                    .foregroundStyle(.blue)
                Divider()
                Text("Centered multiline text that needs to wrap across lines")
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: .infinity)
            }
            .padding()
        }
        .navigationTitle("Typography")
        .navigationBarTitleDisplayMode(.inline)
    }
}

struct ShapesScreen: View {
    var body: some View {
        VStack(spacing: 20) {
            HStack(spacing: 16) {
                Circle().fill(.red).frame(width: 60, height: 60)
                Capsule().fill(.green).frame(width: 100, height: 44)
                RoundedRectangle(cornerRadius: 12).fill(.blue).frame(width: 60, height: 60)
            }
            RoundedRectangle(cornerRadius: 16)
                .stroke(.orange, lineWidth: 4)
                .frame(height: 80)
                .overlay {
                    Text("Overlay").font(.headline)
                }
            Text("Shadow card")
                .padding()
                .background(Color.white)
                .cornerRadius(12)
                .shadow(radius: 6)
            ZStack {
                Circle().fill(Color.purple.opacity(0.3))
                Circle().stroke(Color.purple, lineWidth: 2).padding(10)
                Text("ZStack").font(.caption)
            }
            .frame(width: 120, height: 120)
            Spacer()
        }
        .padding()
        .navigationTitle("Shapes")
    }
}

struct TodoScreen: View {
    @ObservedObject var store: TodoStore
    @State private var draft = ""

    var body: some View {
        List {
            Section {
                HStack {
                    TextField("New todo", text: $draft)
                    Button("Add") {
                        store.add(draft)
                        draft = ""
                    }
                    .disabled(draft.isEmpty)
                }
            }
            Section("Items") {
                ForEach(store.items.indices, id: \.self) { index in
                    TodoRow(item: $store.items[index])
                }
            }
        }
        .navigationTitle("Todos")
    }
}

struct TodoRow: View {
    @Binding var item: TodoItem

    var body: some View {
        Button {
            item.done.toggle()
        } label: {
            HStack(spacing: 12) {
                Circle()
                    .fill(item.done ? Color.green : Color.clear)
                    .overlay { Circle().stroke(Color.gray, lineWidth: 1.5) }
                    .frame(width: 22, height: 22)
                Text(item.title)
                    .strikethrough(item.done)
                    .foregroundStyle(item.done ? .secondary : .primary)
                Spacer()
            }
        }
    }
}

struct ControlsScreen: View {
    @State private var notifications = true
    @State private var volume = 3

    var body: some View {
        List {
            Toggle("Notifications", isOn: $notifications)
            Stepper("Volume: \(volume)", value: $volume, in: 0...10)
            CounterRow(value: $volume)
            if notifications {
                Text("You will be notified")
                    .foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Controls")
    }
}

struct CounterRow: View {
    @Binding var value: Int

    var body: some View {
        HStack {
            Button("Less") { value -= 1 }
                .buttonStyle(.bordered)
            Text("\(value)")
                .frame(minWidth: 40)
            Button("More") { value += 1 }
                .buttonStyle(.borderedProminent)
        }
    }
}

struct SymbolsScreen: View {
    var body: some View {
        List {
            Section("Labels") {
                Label("Favorites", systemImage: "star.fill")
                Label("Settings", systemImage: "gear")
                Label("Trash", systemImage: "trash")
                    .foregroundStyle(.red)
            }
            Section("Images") {
                HStack(spacing: 20) {
                    Image(systemName: "heart.fill")
                        .foregroundStyle(.pink)
                    Image(systemName: "bell")
                        .font(.title)
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundStyle(.green)
                        .imageScale(.large)
                    Image(systemName: "paperplane")
                        .font(.system(size: 28, weight: .bold))
                    Image(systemName: "cloud.sun.fill")
                        .resizable()
                        .scaledToFit()
                        .frame(width: 44, height: 44)
                }
            }
            Section("Inline") {
                HStack {
                    Image(systemName: "envelope")
                    Text("Messages")
                    Spacer()
                    Text("12").foregroundStyle(.secondary)
                    Image(systemName: "chevron.right")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(.tertiary)
                }
            }
        }
        .navigationTitle("Symbols")
    }
}

struct GlassScreen: View {
    @State private var taps = 0

    var body: some View {
        ZStack {
            VStack(spacing: 0) {
                Color.orange
                Color.pink
                Color.purple
                Color.blue
                Color.green
            }
            VStack(spacing: 24) {
                Text("Liquid Glass")
                    .font(.title.weight(.semibold))
                    .padding(.horizontal, 24)
                    .padding(.vertical, 12)
                    .glassEffect()
                HStack(spacing: 16) {
                    Image(systemName: "star.fill")
                        .font(.title2)
                        .frame(width: 56, height: 56)
                        .glassEffect(in: .circle)
                    Image(systemName: "heart.fill")
                        .font(.title2)
                        .frame(width: 56, height: 56)
                        .glassEffect(.regular.tint(.red), in: .circle)
                    Image(systemName: "bell.fill")
                        .font(.title2)
                        .frame(width: 56, height: 56)
                        .glassEffect(.clear, in: .circle)
                }
                Text("Card")
                    .frame(width: 220, height: 64)
                    .glassEffect(in: .rect(cornerRadius: 20))
                Button("Glass \(taps)") { taps += 1 }
                    .buttonStyle(.glass)
                Button("Prominent") { taps += 1 }
                    .buttonStyle(.glassProminent)
            }
        }
        .navigationTitle("Glass")
    }
}
