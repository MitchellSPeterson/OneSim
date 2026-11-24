import Foundation
import SwiftUI

// Wire-test app for accessibility modifiers. Semantics are on the guest tree. Not a visual reference.

private struct AxPerson: Identifiable {
    let id: Int
    let name: String
}

private struct AxRow {
    let key: String
    let name: String
}

private struct AxNamed: AccessibilityRotorContent {
    var body: some AccessibilityRotorContent {
        AccessibilityRotorEntry("Ada", id: "body")
    }
}

private struct SalesChart: AXChartDescriptorRepresentable {
    func makeChartDescriptor() -> AXChartDescriptor {
        let d = AXChartDescriptor(title: "Sales", summary: "Up")
        updateChartDescriptor(d)
        return d
    }
}

struct AccessibilityLab: View {
    private enum Field: Hashable { case main }

    @State private var count = 0
    @State private var adjust = ""
    @State private var edge = ""
    @State private var zoom = ""
    @AccessibilityFocusState private var voice: Bool
    @AccessibilityFocusState private var which: Field?
    @AccessibilityFocusState(for: .voiceOver) private var tech: Bool
    @AccessibilityFocusState(for: [.voiceOver, .switchControl]) private var both: Field?
    @Namespace private var ns

    var body: some View {
        VStack {
            Text("Alpha")
                .accessibilityHint(Text("Hint"))
                .accessibilityValue(LocalizedStringKey("Val"))
                .accessibilityIdentifier("alpha")
                .accessibilityLabel("Outer")
            Text("Beta")
                .accessibilityHidden(true)
            VStack {
                Text("One")
                Text("Two")
            }
            .accessibilityElement(children: .combine)
            Text("Go")
                .accessibilityActions {
                    Button("Run") { count += 1 }
                }
            VStack {
                Text("Count \(count)")
                    .accessibilityAction { count += 1 }
                Text("Adjust \(adjust)")
                    .accessibilityAdjustableAction { adjust = $0 == .increment ? "inc" : "dec" }
                Text("Edge \(edge)")
                    .accessibilityScrollAction { e in
                        switch e {
                        case .top: edge = "top"
                        case .leading: edge = "leading"
                        case .bottom: edge = "bottom"
                        case .trailing: edge = "trailing"
                        }
                    }
                Text("Zoom \(zoom)")
                    .accessibilityZoomAction { z in
                        zoom = z.direction == .zoomIn && z.location == .center && z.point == .zero ? "in" : "out"
                    }
                Text("Focus me")
                    .accessibilityFocused($voice)
                    .accessibilityDefaultFocus($voice, true)
                Text("Which")
                    .accessibilityFocused($which, equals: .main)
                Text("Tech")
                    .accessibilityFocused($tech)
                Text("Both")
                    .accessibilityFocused($both, equals: .main)
                    .accessibilityDefaultFocus($both, .main)
                Text("Off")
                    .accessibilityLabel("Nope", isEnabled: false)
                    .accessibilityHint("Nope", isEnabled: false)
                VStack {
                    Text("Name")
                        .accessibilityLabeledPair(role: .label, id: "pair", in: ns)
                    Text("Value")
                        .accessibilityLabeledPair(role: .content, id: "pair", in: ns)
                }
            }
            content(ns)
            rotors(ns)
            points()
            deprecatedMarks()
        }
    }

    private func content(_ ns: Namespace.ID) -> some View {
        let sub: Substring = "Sub"[...]
        let note = AccessibilityCustomContentKey(Text("Note"), id: "note")
        let keyed = AccessibilityCustomContentKey(LocalizedStringKey("K"), id: "k")
        let bare = AccessibilityCustomContentKey("Bare")
        let resKey = AccessibilityCustomContentKey(LocalizedStringResource("ResKey"), id: "rk")
        let resBare = AccessibilityCustomContentKey(LocalizedStringResource("ResBare"))
        let interpolated: LocalizedStringKey = "Count \(2)"
        let attachment = AccessibilityAttachmentModifier()
        let _ = attachment
        return Text("Extra")
            .accessibilityLabel(Text("TLabel"))
            .accessibilityLabel(LocalizedStringKey("KLabel"))
            .accessibilityLabel(sub)
            .accessibilityLabel(LocalizedStringResource("RLabel"))
            .accessibilityLabel { _ in Text("Built") }
            .accessibilityLabel("Skipped", isEnabled: false)
            .accessibilityValue(Text("TValue"))
            .accessibilityValue(interpolated)
            .accessibilityValue(sub)
            .accessibilityValue(LocalizedStringResource("RValue"))
            .accessibilityHint(Text("THint"))
            .accessibilityHint(LocalizedStringKey("KHint"))
            .accessibilityHint(sub)
            .accessibilityHint(LocalizedStringResource("RHint"))
            .accessibilityInputLabels([Text("Alpha"), Text("A")])
            .accessibilityInputLabels([LocalizedStringKey("C")])
            .accessibilityInputLabels([sub])
            .accessibilitySortPriority(5)
            .accessibilityHeading(.h1)
            .accessibilityAddTraits([.isButton, .isHeader])
            .accessibilityRemoveTraits(.isImage)
            .accessibilityTextContentType(.sourceCode)
            .accessibilityElement()
            .accessibilityElement(children: .contain)
            .accessibilityScrollStatus(Text("At top"))
            .accessibilityScrollStatus(LocalizedStringKey("Keyed status"))
            .accessibilityScrollStatus(sub)
            .accessibilityScrollStatus(LocalizedStringResource("Res status"))
            .accessibilityAction(.escape) { count += 1 }
            .accessibilityAction(.magicTap) { count += 1 }
            .accessibilityAction(.delete) { count += 1 }
            .accessibilityAction(.showMenu) { count += 1 }
            .accessibilityAction(named: Text("Named")) { count += 1 }
            .accessibilityAction(named: LocalizedStringKey("KeyedAct")) { count += 1 }
            .accessibilityAction(named: sub) { count += 1 }
            .accessibilityAction(named: LocalizedStringResource("ResAct")) { count += 1 }
            .accessibilityAction(action: { count += 1 }) { Text("Labeled") }
            .accessibilityActions(category: .edit) { Button("Edit") { count += 1 } }
            .accessibilityActions(category: AccessibilityActionCategory(Text("Custom"))) { Button("Custom") { count += 1 } }
            .accessibilityActions(category: AccessibilityActionCategory(LocalizedStringKey("KeyedCat"))) { Button("Keyed") { count += 1 } }
            .accessibilityActions(category: AccessibilityActionCategory(sub)) { Button("Sub") { count += 1 } }
            .accessibilityActions(category: AccessibilityActionCategory(LocalizedStringResource("ResCat"))) { Button("Res") { count += 1 } }
            .accessibilityDirectTouch()
            .accessibilityDirectTouch(true, options: [.silentOnTouch, .requiresActivation])
            .accessibilityRespondsToUserInteraction(false)
            .accessibilityRespondsToUserInteraction(true, isEnabled: false)
            .accessibilityRespondsToUserInteraction(true, isEnabled: true)
            .accessibilityIgnoresInvertColors()
            .accessibilityCustomContent(note, Text("Hello"), importance: .high)
            .accessibilityCustomContent(keyed, LocalizedStringKey("KeyedVal"))
            .accessibilityCustomContent(note, nil as Text?)
            .accessibilityCustomContent(Text("Rank"), Text("High"))
            .accessibilityCustomContent(LocalizedStringKey("RankKey"), Text("High"))
            .accessibilityCustomContent(LocalizedStringKey("RankBoth"), LocalizedStringKey("High"))
            .accessibilityCustomContent(bare, sub)
            .accessibilityCustomContent(resKey, LocalizedStringResource("ResVal"), importance: .default)
            .accessibilityCustomContent(resBare, Text("BareVal"))
            .accessibilityCustomContent(LocalizedStringResource("ResLabel"), Text("ResText"))
            .accessibilityCustomContent(LocalizedStringResource("ResLabel2"), LocalizedStringResource("ResVal2"))
            .accessibilityCustomContent(LocalizedStringKey("PairKey"), sub)
            .accessibilityCustomContent(sub, sub)
            .accessibilityCustomContent(LocalizedStringResource("ResPair"), sub)
            .accessibilityRepresentation { Text("Rep") }
            .accessibilityChildren { Text("KidA"); Text("KidB") }
            .accessibilityShowsLargeContentViewer()
            .accessibilityShowsLargeContentViewer { Text("Big") }
            .accessibilityChartDescriptor(SalesChart())
            .accessibilityLinkedGroup(id: "g1", in: ns)
            .accessibilityRotorEntry(id: "re", in: ns)
            .accessibilityIdentifier("extra")
            .accessibilityHidden(false)
            .accessibilityIdentifier("skip-id", isEnabled: false)
    }

    private func rotors(_ ns: Namespace.ID) -> some View {
        let sample = "Hello"
        let range = sample.startIndex..<sample.index(after: sample.startIndex)
        let ranges = [range]
        let sub: Substring = "SubRotor"[...]
        let people = [AxPerson(id: 7, name: "Ann")]
        let rows = [AxRow(key: "a", name: "Ada")]
        return rotorRanges(rotorArrays(rotorBuilders(Text("Rotors"), ns, range, sub), people, rows, sub), ranges, sub)
    }

    private func rotorBuilders<V: View>(_ view: V, _ ns: Namespace.ID, _ range: Range<String.Index>, _ sub: Substring) -> some View {
        let show = true
        return view
            .accessibilityRotor(Text("Built")) {
                AccessibilityRotorEntry(Text("Ann"), id: 1, prepare: {})
                AccessibilityRotorEntry(Text("In"), id: "e", in: ns)
                if show { AccessibilityRotorEntry(LocalizedStringKey("Bo"), id: 2) }
            }
            .accessibilityRotor(.links(visited: true)) {
                if show {
                    AccessibilityRotorEntry(sub, id: "s")
                } else {
                    AccessibilityRotorEntry(Text("No"), id: "n")
                }
            }
            .accessibilityRotor(LocalizedStringKey("Either")) {
                AccessibilityRotorEntry(LocalizedStringKey("Nk"), id: "nk", in: ns)
                AccessibilityRotorEntry(sub, id: "sn", in: ns)
                AccessibilityRotorEntry(LocalizedStringResource("Rn"), id: "rn", in: ns, textRange: range)
            }
            .accessibilityRotor(sub) {
                if #available(macOS 13, *) {
                    AccessibilityRotorEntry(Text("Avail"), id: "av")
                }
            }
            .accessibilityRotor(LocalizedStringResource("ResRotor")) {
                AccessibilityRotorEntry(LocalizedStringResource("ResEntry"), id: "r", textRange: range)
                AccessibilityRotorEntry(textRange: range)
            }
            .accessibilityRotor(Text("Empty")) {}
            .accessibilityRotor(.headings(level: .h2)) {
                AccessibilityRotorEntry(Text("Span"), id: 9, textRange: range)
            }
            .accessibilityRotor(.links) { AccessibilityRotorEntry("Yes", id: "y") }
            .accessibilityRotor(.headings) { AccessibilityRotorEntry(Text("H"), id: "h") }
            .accessibilityRotor(Text("More")) {
                AccessibilityRotorEntry(LocalizedStringKey("Only"), textRange: range)
                AccessibilityRotorEntry(sub, textRange: range)
                AccessibilityRotorEntry(LocalizedStringResource("ResRange"), textRange: range)
                AccessibilityRotorEntry(sub, "uid", in: ns)
                AxNamed()
            }
    }

    private func rotorArrays<V: View>(_ view: V, _ people: [AxPerson], _ rows: [AxRow], _ sub: Substring) -> some View {
        view
            .accessibilityRotor(Text("People"), entries: people, entryLabel: \.name)
            .accessibilityRotor(Text("Rows"), entries: rows, entryID: \.key, entryLabel: \.name)
            .accessibilityRotor(.images, entries: people, entryLabel: \.name)
            .accessibilityRotor(.textFields, entries: rows, entryID: \.key, entryLabel: \.name)
            .accessibilityRotor(LocalizedStringKey("PeopleKey"), entries: people, entryLabel: \.name)
            .accessibilityRotor(sub, entries: people, entryLabel: \.name)
            .accessibilityRotor(LocalizedStringKey("RowsKey"), entries: rows, entryID: \.key, entryLabel: \.name)
            .accessibilityRotor(sub, entries: rows, entryID: \.key, entryLabel: \.name)
            .accessibilityRotor(LocalizedStringResource("PeopleRes"), entries: people, entryLabel: \.name)
            .accessibilityRotor(LocalizedStringResource("RowsRes"), entries: rows, entryID: \.key, entryLabel: \.name)
    }

    private func rotorRanges<V: View>(_ view: V, _ ranges: [Range<String.Index>], _ sub: Substring) -> some View {
        view
            .accessibilityRotor(Text("Spans"), textRanges: ranges)
            .accessibilityRotor(.boldText, textRanges: ranges)
            .accessibilityRotor(LocalizedStringKey("SpansKey"), textRanges: ranges)
            .accessibilityRotor(sub, textRanges: ranges)
            .accessibilityRotor(LocalizedStringResource("SpansRes"), textRanges: ranges)
            .accessibilityRotor(.italicText, textRanges: ranges)
            .accessibilityRotor(.underlineText, textRanges: ranges)
            .accessibilityRotor(.misspelledWords, textRanges: ranges)
            .accessibilityRotor(.tables, textRanges: ranges)
            .accessibilityRotor(.lists, textRanges: ranges)
            .accessibilityRotor(.landmarks, textRanges: ranges)
    }

    private func points() -> some View {
        let sub: Substring = "SubDrag"[...]
        let pt = CGPoint(x: 8, y: 9)
        let up = UnitPoint(x: 0.25, y: 0.75)
        return Text("Points")
            .accessibilityActivationPoint(CGPoint(x: 3, y: 4))
            .accessibilityActivationPoint(UnitPoint(x: 0, y: 1))
            .accessibilityActivationPoint(CGPoint(x: 1, y: 1), isEnabled: false)
            .accessibilityDragPoint(up, description: Text("Drag me"))
            .accessibilityDragPoint(up, description: LocalizedStringKey("DragKey"))
            .accessibilityDragPoint(up, description: sub)
            .accessibilityDragPoint(pt, description: Text("DragAbs"))
            .accessibilityDragPoint(pt, description: LocalizedStringKey("DragAbsKey"))
            .accessibilityDragPoint(pt, description: sub)
            .accessibilityDragPoint(up, description: LocalizedStringResource("DragRes"))
            .accessibilityDragPoint(pt, description: LocalizedStringResource("DragAbsRes"))
            .accessibilityDropPoint(up, description: Text("Drop me"))
            .accessibilityDropPoint(up, description: LocalizedStringKey("DropKey"))
            .accessibilityDropPoint(up, description: sub)
            .accessibilityDropPoint(pt, description: Text("DropAbs"))
            .accessibilityDropPoint(pt, description: LocalizedStringKey("DropAbsKey"))
            .accessibilityDropPoint(pt, description: sub)
            .accessibilityDropPoint(up, description: LocalizedStringResource("DropRes"))
            .accessibilityDropPoint(pt, description: LocalizedStringResource("DropAbsRes"))
    }

    private func deprecatedMarks() -> some View {
        Text("Old")
            .accessibility(label: Text("Old"))
            .accessibility(value: Text("OldVal"))
            .accessibility(hint: Text("OldHint"))
            .accessibility(inputLabels: [Text("OldIn")])
            .accessibility(identifier: "old-id")
            .accessibility(selectionIdentifier: "sel")
            .accessibility(hidden: false)
            .accessibility(sortPriority: 1)
            .accessibility(activationPoint: CGPoint(x: 1, y: 2))
            .accessibility(activationPoint: UnitPoint.center)
            .accessibility(addTraits: .isStaticText)
            .accessibility(removeTraits: .isModal)
    }
}
