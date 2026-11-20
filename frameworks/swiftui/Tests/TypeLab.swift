import SwiftUI
import Foundation

// Wire lab: text case, truncation, a hierarchical symbol, an empty AsyncImage, and a file-URL bitmap.

private enum LabPNG {
    static let bytes: [UInt8] = [
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 8, 0, 0, 0, 8, 8, 2, 0, 0, 0, 75, 109, 41, 220,
        0, 0, 0, 16, 73, 68, 65, 84, 120, 218, 99, 96, 168, 250, 143, 29, 13, 45, 9, 0, 148, 36, 94, 65, 154, 232, 144, 245,
        0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ]
    static func install() -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("onesim-typelab.png")
        try? Data(bytes).write(to: url)
        return url
    }
}

@main
struct TypeLabApp: App {
    var body: some Scene {
        WindowGroup { TypeLab() }
    }
}

struct TypeLab: View {
    private let sample: URL
    init() { sample = LabPNG.install() }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Hello").textCase(.uppercase)
            Text("A").italic() + Text("B").kerning(2)
            (Text("Dot").underline(true, pattern: .dot, color: nil) + Text("End"))
            Text("The quick brown fox jumps over the lazy dog").lineLimit(1).truncationMode(.middle)
            Image(systemName: "star.fill").symbolRenderingMode(.hierarchical)
            AsyncImage(url: nil)
            AsyncImage(url: sample)
        }
    }
}
