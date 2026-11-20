import SwiftUI
import Foundation

// Text and image styling. Builds unchanged against Apple's SwiftUI (iOS 26+) and OneSim.

private let longLine = "The quick brown fox jumps over the lazy dog"

private enum SamplePNG {
    // 8x8 solid blue.
    static let bytes: [UInt8] = [
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 8, 0, 0, 0, 8, 8, 2, 0, 0, 0, 75, 109, 41, 220,
        0, 0, 0, 16, 73, 68, 65, 84, 120, 218, 99, 96, 168, 250, 143, 29, 13, 45, 9, 0, 148, 36, 94, 65, 154, 232, 144, 245,
        0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ]
    static func install() -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("onesim-type.png")
        try? Data(bytes).write(to: url)
        return url
    }
}

@main
struct TypeApp: App {
    var body: some Scene {
        WindowGroup { TypeScreen() }
    }
}

struct TypeScreen: View {
    private let sample: URL
    init() { sample = SamplePNG.install() }

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 8) {
                VStack(alignment: .leading, spacing: 8) {
                    labeled("Tracking") {
                        HStack(spacing: 10) {
                            Text("Wide").tracking(4)
                            Text("Plain")
                        }
                    }
                    labeled("Kerning") { Text("AVATAR").kerning(4) }
                    labeled("Baseline") {
                        HStack(spacing: 8) {
                            Text("Up").baselineOffset(6)
                            Text("Base")
                        }
                    }
                    labeled("Design") {
                        HStack(spacing: 8) {
                            Text("Serif").font(.system(size: 17, design: .serif))
                            Text("Round").fontDesign(.rounded)
                        }
                    }
                    labeled("Width") { Text("Condensed").fontWidth(.condensed) }
                    labeled("Digits") {
                        VStack(alignment: .leading, spacing: 0) {
                            Text("00000000").monospacedDigit()
                            Text("11111111").monospacedDigit()
                        }
                    }
                    labeled("Scale") { Text("Secondary").textScale(.secondary) }
                }
                VStack(alignment: .leading, spacing: 8) {
                    labeled("Head") { Text(longLine).lineLimit(1).truncationMode(.head).frame(width: 160) }
                    labeled("Tail") { Text(longLine).lineLimit(1).truncationMode(.tail).frame(width: 160) }
                    labeled("Middle") { Text(longLine).lineLimit(1).truncationMode(.middle).frame(width: 160) }
                    labeled("Underline") { Text("Dotted").underline(pattern: .dot) }
                    labeled("Case") { Text("Upper").textCase(.uppercase) }
                    labeled("Symbols") {
                        HStack(spacing: 12) {
                            Image(systemName: "star.fill").symbolRenderingMode(.monochrome)
                            Image(systemName: "star.fill").symbolRenderingMode(.hierarchical)
                            Image(systemName: "star.fill").symbolRenderingMode(.multicolor)
                        }
                    }
                    labeled("Async") { AsyncImage(url: sample).frame(width: 32, height: 32) }
                }
            }
            .padding(16)
            .navigationTitle("Type")
        }
    }
}

private func labeled<V: View>(_ title: String, @ViewBuilder content: () -> V) -> some View {
    HStack(spacing: 8) {
        Text(title).font(.caption).foregroundStyle(.secondary).frame(width: 76, alignment: .leading)
        content()
    }
}
