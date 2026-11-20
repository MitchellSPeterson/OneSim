import SwiftUI

// Wire-test app for controls (protocol section O): Menu, Link and openURL, toggle / button / progress / gauge / menu
// styles (built-in and custom), Slider value labels and onEditingChanged, and the inline, navigationLink and wheel
// picker styles.

struct StyledButton: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label.padding(4).background(Color.orange)
    }
}

/// Runs the action from a tap on the label (`trigger()`).
struct TapTrigger: PrimitiveButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label.onTapGesture { configuration.trigger() }
    }
}

struct Checkbox: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        Button { configuration.isOn.toggle() } label: {
            HStack {
                Image(systemName: configuration.isOn ? "checkmark.square.fill" : "square")
                configuration.label
            }
        }
    }
}

struct Percent: ProgressViewStyle {
    func makeBody(configuration: Configuration) -> some View {
        Text("pct \(Int((configuration.fractionCompleted ?? 0) * 100))")
    }
}

struct GaugeText: GaugeStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack {
            configuration.label
            Text("G \(configuration.value)")
        }
    }
}

struct Faded: MenuStyle {
    func makeBody(configuration: Configuration) -> some View {
        Menu(configuration).opacity(0.5)
    }
}

struct ControlLab: View {
    @State private var last = "none"
    @State private var opened = "none"
    @State private var tb = false
    @State private var check = false
    @State private var styled = 0
    @State private var prim = 0
    @State private var level = 0.0
    @State private var edits: [Bool] = []
    @State private var pick = 1
    @State private var size = "S"
    @State private var hour = 1
    @State private var flavor = "a"
    @State private var mpick = 1

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Menu("Menu") {
                        Button("Alpha", systemImage: "star") { last = "Alpha" }
                        Divider()
                        Button("Delete", role: .destructive) { last = "Delete" }
                        Section { Button("Inside") { last = "Inside" } }
                        Button("Off") { last = "Off" }.disabled(true)
                        Toggle("Flag", isOn: $tb)
                        Picker("Pick", selection: $mpick) {
                            Text("P1").tag(1)
                            Text("P2").tag(2)
                        }
                    }
                    Menu {
                        Button("Never") { last = "Never" }
                    } label: {
                        Text("Primary")
                    } primaryAction: {
                        last = "Primary"
                    }
                    Menu("Faded") { Button("Beta") { last = "Beta" } }
                        .menuStyle(Faded())
                    Text("last \(last) mpick \(mpick)")
                    Link("Docs", destination: URL(string: "https://docs.test")!)
                        .environment(\.openURL, OpenURLAction { url in opened = url.absoluteString; return .handled })
                    Link("Plain", destination: URL(string: "https://plain.test")!)
                    Link("Dropped", destination: URL(string: "https://dropped.test")!)
                        .environment(\.openURL, OpenURLAction { _ in .discarded })
                    Link("Redirect", destination: URL(string: "https://from.test")!)
                        .environment(\.openURL, OpenURLAction { _ in .systemAction(URL(string: "https://to.test")!) })
                    Text("opened \(opened)")
                }
                Section {
                    Toggle("TB", isOn: $tb).toggleStyle(.button)
                    Toggle("Check", isOn: $check).toggleStyle(Checkbox())
                    Button("Styled") { styled += 1 }.buttonStyle(StyledButton())
                    Button("Prim") { prim += 1 }.buttonStyle(TapTrigger())
                    Text("styled \(styled) prim \(prim)")
                }
                Section {
                    Slider(value: $level, in: 0...1) {
                        Text("Level")
                    } minimumValueLabel: {
                        Text("Lo")
                    } maximumValueLabel: {
                        Text("Hi")
                    } onEditingChanged: { edits.append($0) }
                    Text("edits \(edits.map { $0 ? "t" : "f" }.joined()) \(level)")
                    ProgressView(value: 0.5).progressViewStyle(Percent())
                    ProgressView(value: 0.5).progressViewStyle(.circular)
                    Gauge(value: 25, in: 0...100) { Text("Fuel") }
                    Gauge(value: 0.25) { Text("Custom") }.gaugeStyle(GaugeText())
                }
                Section {
                    Picker("Inline", selection: $pick) {
                        Text("One").tag(1)
                        Text("Two").tag(2)
                    }
                    .pickerStyle(.inline)
                    Picker("Size", selection: $size) {
                        Text("S").tag("S")
                        Text("L").tag("L")
                    }
                    .pickerStyle(.navigationLink)
                    Picker("Hour", selection: $hour) {
                        ForEach(0..<4) { Text("h\($0)").tag($0) }
                    }
                    .pickerStyle(.wheel)
                    Group {
                        Picker("Flavor", selection: $flavor) {
                            Text("a").tag("a")
                        }
                        Toggle("Hidden", isOn: $check)
                    }
                    .labelsHidden()
                    Text("pick \(pick) size \(size) hour \(hour)")
                }
            }
        }
    }
}
