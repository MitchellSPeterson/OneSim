import SwiftUI

// Controls: Menu, Link, button and toggle styles (built-in and custom), picker styles, Slider value labels,
// ProgressView styles and Gauge styles. Builds unchanged against Apple's SwiftUI (scripts/build-ios-sim.sh);
// the reference states are in tests/fixtures/swiftui-controls-ios/ (iOS 26.5) and
// tests/fixtures/ios27/swiftui-controls-ios/ (iOS 27.2).

@main
struct ControlsApp: App {
    var body: some Scene {
        WindowGroup { ControlsView() }
    }
}

/// A capsule button that darkens while pressed.
struct PillButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .foregroundStyle(Color.white)
            .padding(.horizontal, 20)
            .padding(.vertical, 10)
            .background(Capsule().fill(configuration.isPressed ? Color.gray : Color.orange))
    }
}

/// A checkbox: a square symbol that fills when on, then the label.
struct CheckboxToggleStyle: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        Button { configuration.isOn.toggle() } label: {
            HStack(spacing: 8) {
                Image(systemName: configuration.isOn ? "checkmark.square.fill" : "square")
                configuration.label
            }
        }
        .buttonStyle(.plain)
    }
}

/// A percentage readout instead of a bar.
struct PercentProgressStyle: ProgressViewStyle {
    func makeBody(configuration: Configuration) -> some View {
        Text("\(Int((configuration.fractionCompleted ?? 0) * 100))% done")
            .font(.headline)
    }
}

struct ControlsView: View {
    @State private var bold = false
    @State private var wifi = true
    @State private var agreed = false
    @State private var pills = 0
    @State private var last = "None"

    var body: some View {
        NavigationStack {
            VStack(spacing: 18) {
                Menu("Actions") {
                    Button("Duplicate", systemImage: "plus.square.on.square") { last = "Duplicate" }
                    Button("Rename", systemImage: "pencil") { last = "Rename" }
                    Divider()
                    Button("Delete", systemImage: "trash", role: .destructive) { last = "Delete" }
                }
                Text("Last action: \(last)")
                Menu {
                    Button("Newest first") { last = "Newest" }
                    Button("Oldest first") { last = "Oldest" }
                } label: {
                    Label("Sort", systemImage: "arrow.up.arrow.down")
                }
                .buttonStyle(.bordered)
                Link("Apple Developer", destination: URL(string: "https://developer.apple.com")!)
                Button("Borderless") { pills += 10 }
                    .buttonStyle(.borderless)
                Button("Pill \(pills)") { pills += 1 }
                    .buttonStyle(PillButtonStyle())
                Toggle("Bold", isOn: $bold)
                    .toggleStyle(.button)
                Toggle("I agree", isOn: $agreed)
                    .toggleStyle(CheckboxToggleStyle())
                Toggle("Wi-Fi", isOn: $wifi)
                    .toggleStyle(.switch)
                    .padding(.horizontal, 40)
                HStack(spacing: 24) {
                    NavigationLink("Pickers") { PickersScreen() }
                    NavigationLink("Indicators") { IndicatorsScreen() }
                }
            }
            .navigationTitle("Controls")
        }
    }
}

struct PickersScreen: View {
    @State private var color = "Red"
    @State private var size = 1
    @State private var shape = "Circle"
    @State private var hour = 9

    var body: some View {
        Form {
            Section("Inline") {
                Picker("Color", selection: $color) {
                    Text("Red").tag("Red")
                    Text("Green").tag("Green")
                    Text("Blue").tag("Blue")
                }
                .pickerStyle(.inline)
                .labelsHidden()
            }
            Section("Navigation link") {
                Picker("Size", selection: $size) {
                    Text("Small").tag(0)
                    Text("Medium").tag(1)
                    Text("Large").tag(2)
                }
                .pickerStyle(.navigationLink)
            }
            Section("Palette") {
                Picker("Shape", selection: $shape) {
                    Text("Circle").tag("Circle")
                    Text("Square").tag("Square")
                    Text("Star").tag("Star")
                }
                .pickerStyle(.palette)
            }
            Section("Wheel") {
                Picker("Hour", selection: $hour) {
                    ForEach(6..<13) { h in Text("\(h):00").tag(h) }
                }
                .pickerStyle(.wheel)
            }
        }
        .navigationTitle("Pickers")
    }
}

struct IndicatorsScreen: View {
    @State private var volume = 0.4

    var body: some View {
        VStack(spacing: 16) {
            ProgressView(value: 0.3) {
                Text("Download")
            } currentValueLabel: {
                Text("30%")
            }
            ProgressView(value: 0.6)
                .progressViewStyle(.circular)
            ProgressView(value: 0.75)
                .progressViewStyle(PercentProgressStyle())
            Slider(value: $volume, in: 0...1) {
                Text("Volume")
            } minimumValueLabel: {
                Image(systemName: "speaker.fill")
            } maximumValueLabel: {
                Image(systemName: "speaker.wave.3.fill")
            }
            Gauge(value: 0.4) {
                Text("Battery")
            } currentValueLabel: {
                Text("40%")
            } minimumValueLabel: {
                Text("0")
            } maximumValueLabel: {
                Text("100")
            }
            Gauge(value: 0.7) { Text("Disk") }
                .gaugeStyle(.linearCapacity)
            Gauge(value: 0.25) { Text("CPU") }
                .gaugeStyle(.accessoryLinearCapacity)
            Gauge(value: 0.5) { Text("Load") }
                .gaugeStyle(.accessoryLinear)
            HStack(spacing: 24) {
                Gauge(value: 0.6) {
                    Text("Ring")
                } currentValueLabel: {
                    Text("60")
                }
                .gaugeStyle(.accessoryCircularCapacity)
                Gauge(value: 0.3) {
                    Text("Arc")
                } currentValueLabel: {
                    Text("30")
                }
                .gaugeStyle(.accessoryCircular)
            }
        }
        .padding(.horizontal, 20)
        .navigationTitle("Indicators")
    }
}
