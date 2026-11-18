import SwiftUI

// Tab views: three tabs, the first a NavigationStack, the second keeps its own @State across tab switches,
// the third carries a badge. Builds unchanged against Apple's SwiftUI (scripts/build-ios-sim.sh).

@main
struct TabsApp: App {
    var body: some Scene {
        WindowGroup { TabsRoot() }
    }
}

struct TabsRoot: View {
    @State private var selection = 0

    var body: some View {
        TabView(selection: $selection) {
            HomeTab()
                .tabItem { Label("Home", systemImage: "house") }
                .tag(0)
            CounterTab()
                .tabItem { Label("Counter", systemImage: "star") }
                .tag(1)
            SettingsTab()
                .tabItem { Label("Settings", systemImage: "gearshape") }
                .badge(3)
                .tag(2)
        }
    }
}

struct HomeTab: View {
    let fruits = ["Apple", "Banana", "Cherry", "Grape", "Mango"]

    var body: some View {
        NavigationStack {
            List {
                ForEach(fruits, id: \.self) { fruit in
                    NavigationLink(fruit) {
                        Text("You picked \(fruit)")
                            .navigationTitle(fruit)
                    }
                }
            }
            .navigationTitle("Home")
        }
    }
}

struct CounterTab: View {
    @State private var count = 0

    var body: some View {
        VStack(spacing: 20) {
            Text("Count: \(count)")
                .font(.largeTitle)
            Button("Increment") { count += 1 }
        }
    }
}

struct SettingsTab: View {
    @State private var notifications = true

    var body: some View {
        NavigationStack {
            List {
                Toggle("Notifications", isOn: $notifications)
                Text("Version 1.0")
            }
            .navigationTitle("Settings")
        }
    }
}
