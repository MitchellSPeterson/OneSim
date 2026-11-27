import SwiftUI

// Wire-test app for layout (protocol section M): grid, lazy grid, view that fits, geometry, and ScrollViewReader
// scrollTo. Jump sends scroll_to_id and does not change @State, so it must not emit a new tree.

struct LayoutLab: View {
    var body: some View {
        ScrollViewReader { proxy in
            VStack {
                Grid(alignment: .leading, horizontalSpacing: 8, verticalSpacing: 4) {
                    GridRow(alignment: .top) {
                        Text("A")
                        Text("B").gridCellColumns(2)
                    }
                    Text("C")
                }
                LazyVGrid(columns: [GridItem(.flexible()), GridItem(.fixed(40))], spacing: 8) {
                    Text("1")
                    Text("2")
                }
                ViewThatFits(in: .horizontal) {
                    Text("Wide")
                    Text("Narrow")
                }
                GeometryReader { geo in
                    Text("\(Int(geo.size.width))")
                }
                .frame(height: 40)
                Button("Jump") { proxy.scrollTo("there") }
                ScrollView {
                    VStack {
                        Text("Top")
                        Text("Target").id("there")
                    }
                }
                .frame(height: 80)
                ScrollTargetProbe()
                HStack {
                    Color.primary.opacity(0.5).opacity(0.5)
                    Color.secondary.opacity(2)
                    Color.accentColor.opacity(0)
                    Color.primary.opacity(-0.5)
                    Color.primary.opacity(.nan)
                }
            }
        }
    }
}

private struct ScrollTargetProbe: View {
    @State private var behavior = AnyScrollTargetBehavior(.paging)

    var body: some View {
        ScrollView { Text("Direct paging") }.scrollTargetBehavior(.paging)
        ScrollView { Text("Erased paging") }.scrollTargetBehavior(AnyScrollTargetBehavior(.paging))
        ScrollView { Text("Mutable behavior") }.scrollTargetBehavior(AnyScrollTargetBehavior(behavior))
        Button("Change behavior") { behavior.base = ViewAlignedScrollTargetBehavior() }
    }
}
