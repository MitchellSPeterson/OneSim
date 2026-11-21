import SwiftUI
import UniformTypeIdentifiers

// Wire checks for popovers, presentation modifiers, context menus, ShareLink and PasteButton (wire_test.py).
struct Row: Identifiable { let id: Int }

struct PresentationLab: View {
    @State private var pop = false
    @State private var bubble = false
    @State private var coverPop = false
    @State private var bare = false
    @State private var lockedPop = false
    @State private var popItem: Row?
    @State private var styled = false
    @State private var plainStyled = false
    @State private var log = "log:"
    @State private var pasted = "pasted:"
    @State private var urls = "urls:"
    @State private var off = false

    var body: some View {
        ScrollView {
            VStack {
                Text(log)
                Group {
                Button("Pop") { pop = true }
                    .popover(isPresented: $pop) { Text("Pop body") }
                Button("Bubble") { bubble = true }
                    .popover(isPresented: $bubble, attachmentAnchor: .point(.bottom), arrowEdge: .bottom) {
                        // The adaptation sits on a child: presentation modifiers lift from anywhere in the content.
                        VStack { Text("Bubble body").presentationCompactAdaptation(.popover) }
                    }
                Button("CoverPop") { coverPop = true }
                    .popover(isPresented: $coverPop) {
                        Text("Cover body").presentationCompactAdaptation(horizontal: .fullScreenCover, vertical: .sheet)
                    }
                Button("Bare") { bare = true }
                    .popover(isPresented: $bare, attachmentAnchor: .rect(.rect(CGRect(x: 1, y: 2, width: 3, height: 4)))) {
                        Text("Bare body").presentationCompactAdaptation(.none).interactiveDismissDisabled()
                    }
                Button("LockedPop") { lockedPop = true }
                    .popover(isPresented: $lockedPop) { Text("Locked body").interactiveDismissDisabled() }
                Button("PopItem") { popItem = Row(id: 7) }
                    .popover(item: $popItem) { r in Text("Item \(r.id)") }
                }
                Group {
                Button("Styled") { styled = true }
                    .sheet(isPresented: $styled) {
                        VStack {
                            Text("Styled body")
                                .presentationBackground(alignment: .bottom) { Color.yellow }
                                .presentationCornerRadius(0)
                        }
                        .presentationDetents([.medium, .large])
                        .presentationBackgroundInteraction(.enabled(upThrough: .medium))
                        .presentationContentInteraction(.scrolls)
                        .presentationSizing(.form.fitted(horizontal: true, vertical: false))
                        .presentationPlacement(.leading)
                    }
                Button("PlainStyled") { plainStyled = true }
                    .sheet(isPresented: $plainStyled) {
                        Text("Plain styled")
                            .presentationBackground(Color.red.opacity(0.5))
                            .presentationCornerRadius(nil)
                            .presentationBackgroundInteraction(.disabled)
                    }
                Text("Menu target")
                    .contextMenu {
                        Button("First", systemImage: "star") { log += " first" }
                        Button("Off") { log += " off" }.disabled(true)
                        Divider()
                        Button("Gone", role: .destructive) { log += " gone" }
                    }
                Button("Tap target") { log += " tapped" }
                    .contextMenu { Button("On button") { log += " onbutton" } }
                Text("Preview target")
                    .contextMenu { Button("Open") {} } preview: { Text("Big preview") }
                Text("Empty target").contextMenu { EmptyView() }
                Text("Legacy target").contextMenu(ContextMenu { Button("Legacy item") {} })
                }
                Group {
                Text("Disabled target").contextMenu { Button("Never") {} }.disabled(true)
                List {
                    ForEach([Row(id: 1), Row(id: 2)]) { r in Text("Row \(r.id)") }
                }
                .contextMenu(forSelectionType: Int.self) { ids in
                    Button("Edit \(ids.sorted().map(String.init).joined())") { log += " edit\(ids.sorted())" }
                } primaryAction: { ids in log += " primary\(ids.sorted())" }
                .frame(height: 200)
                ShareLink(item: "share me", subject: Text("Subj"), message: Text("Msg"))
                ShareLink("Many", items: ["a", "b"], preview: { s in SharePreview("Preview \(s)") })
                ShareLink(items: [String]()) { Text("Nothing") }
                PasteButton(payloadType: String.self) { s in pasted += " " + s.joined(separator: ",") }
                PasteButton(payloadType: URL.self) { u in urls += " " + u.map(\.absoluteString).joined() }
                PasteButton(supportedContentTypes: [.plainText]) { providers in pasted += " providers\(providers.count)" }
                Text(pasted)
                Text(urls)
                }
            }
        }
    }
}
