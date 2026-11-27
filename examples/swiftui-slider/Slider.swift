import SwiftUI

@main
struct SliderApp: App {
    var body: some Scene {
        WindowGroup { SliderDemo() }
    }
}

struct SliderDemo: View {
    @State private var playback = -0.5
    @State private var temperature = 1.0
    @State private var volume = 4.0
    @State private var gain = 3.0
    @State private var edits = ""

    var body: some View {
        VStack(spacing: 22) {
            Slider(value: $playback, in: -1...1, neutralValue: 0, enabledBounds: -0.75...0.75) {
                Text("Playback")
            } currentValueLabel: {
                Text("Half speed")
            } minimumValueLabel: {
                Text("Slow")
            } maximumValueLabel: {
                Text("Fast")
            } ticks: {
                SliderTick(-0.75)
                SliderTick("Normal", 0)
                SliderTick(0.75)
            } onEditingChanged: {
                edits += $0 ? "t" : "f"
            }
            .tint(.orange)
            Slider(value: $temperature, in: -4...4, neutralValue: 0, enabledBounds: -3...3) {
                Text("Temperature")
            }
            Slider(value: $volume, in: 0...10, step: 2) {
                Text("Volume")
            } tick: { value in
                value == 4 ? SliderTick("Medium", value) : nil
            }
            Slider(value: $gain, in: 0...10, step: 1)
            Text("Edits: \(edits)")
            Text("Playback: \(playback)")
        }
        .padding(.horizontal, 28)
    }
}
