import SwiftUI

// Layout: grid, lazy grid, the first child that fits, a bounded geometry reader, and a paging horizontal scroll.
// Builds unchanged against Apple's SwiftUI.

@main
struct LayoutApp: App {
    var body: some Scene {
        WindowGroup { LayoutView() }
    }
}

struct LayoutView: View {
    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 12) {
                Grid(alignment: .leading, horizontalSpacing: 8, verticalSpacing: 8) {
                    GridRow {
                        cell("A", r: 0.95, g: 0.55, b: 0.20)
                        cell("B", r: 0.20, g: 0.55, b: 0.95)
                        cell("C", r: 0.20, g: 0.72, b: 0.45)
                    }
                    GridRow {
                        cell("D", r: 0.85, g: 0.28, b: 0.42)
                        cell("E", r: 0.55, g: 0.35, b: 0.85).gridCellColumns(2)
                    }
                }
                LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 8) {
                    ForEach(["1", "2", "3", "4"], id: \.self) { label in
                        tile(label)
                    }
                }
                ViewThatFits {
                    HStack { Text("Wide layout does not fit this row") }
                        .frame(minWidth: 420)
                    Text("Compact")
                }
                GeometryReader { geo in
                    Color.blue.opacity(0.2).overlay(Text("\(Int(geo.size.width))"))
                }
                .frame(height: 80)
                ScrollView(.horizontal) {
                    HStack(spacing: 0) {
                        page("One")
                        page("Two")
                        page("Three")
                    }
                    .scrollTargetLayout()
                }
                .scrollTargetBehavior(.viewAligned)
                .frame(height: 72)
            }
            .padding(16)
            .navigationTitle("Layout")
        }
    }
}

private func cell(_ name: String, r: Double, g: Double, b: Double) -> some View {
    Color(red: r, green: g, blue: b)
        .frame(width: 64, height: 36)
        .overlay(Text(name))
}

private func tile(_ label: String) -> some View {
    let colors = [
        Color(red: 0.90, green: 0.35, blue: 0.30),
        Color(red: 0.25, green: 0.65, blue: 0.45),
        Color(red: 0.25, green: 0.45, blue: 0.90),
        Color(red: 0.70, green: 0.40, blue: 0.85),
    ]
    let i = (Int(label) ?? 1) - 1
    return Text(label)
        .frame(maxWidth: .infinity)
        .frame(height: 36)
        .background(colors[i % colors.count])
}

private func page(_ title: String) -> some View {
    let color = title == "One" ? Color(red: 0.20, green: 0.45, blue: 0.85)
        : title == "Two" ? Color(red: 0.15, green: 0.62, blue: 0.55)
        : Color(red: 0.90, green: 0.48, blue: 0.22)
    return Text(title)
        .frame(width: 320, height: 72)
        .background(color)
}
