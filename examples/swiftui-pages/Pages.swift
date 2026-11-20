import SwiftUI

// TabView styles: a `.sidebarAdaptable` tab bar (a tab bar on iPhone) with Int and String badges; its first tab holds a
// `.page` TabView whose selection is shown above it and moved by each page's button. Builds unchanged against Apple's
// SwiftUI (scripts/build-ios-sim.sh).

@main
struct PagesApp: App {
    var body: some Scene {
        WindowGroup { PagesRoot() }
    }
}

struct PagesRoot: View {
    @State private var tab = 0

    var body: some View {
        TabView(selection: $tab) {
            Tab("Pages", systemImage: "book", value: 0) {
                PagesTab()
            }
            Tab("Inbox", systemImage: "tray", value: 1) {
                Text("Inbox")
                    .font(.largeTitle)
            }
            .badge("New")
            Tab("Alerts", systemImage: "bell", value: 2) {
                Text("Alerts")
                    .font(.largeTitle)
            }
            .badge(12)
        }
        .tabViewStyle(.sidebarAdaptable)
    }
}

struct PagesTab: View {
    @State private var page = 0

    var body: some View {
        VStack(spacing: 12) {
            Text("Page \(page + 1) of 3")
                .font(.headline)
            TabView(selection: $page) {
                PageCard(title: "Sunrise", color: .orange, button: "Next") { page = 1 }
                    .tag(0)
                PageCard(title: "Forest", color: .green, button: "Next") { page = 2 }
                    .tag(1)
                PageCard(title: "Night", color: .purple, button: "Start over") { page = 0 }
                    .tag(2)
            }
            .tabViewStyle(.page)
            .indexViewStyle(.page(backgroundDisplayMode: .always))
            .frame(height: 360)
        }
    }
}

struct PageCard: View {
    let title: String
    let color: Color
    let button: String
    let action: () -> Void

    var body: some View {
        VStack(spacing: 16) {
            RoundedRectangle(cornerRadius: 24)
                .fill(color)
                .frame(width: 220, height: 160)
            Text(title)
                .font(.title)
            Button(button, action: action)
        }
    }
}
