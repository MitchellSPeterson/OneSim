import SwiftUI

// Gestures: drag with @GestureState, long press, double tap, and a simultaneous pinch + rotate.
// Builds unchanged against Apple's SwiftUI (scripts/build-ios-sim.sh); the reference states are in
// tests/fixtures/gestures-ios/ (iOS 26.5) and tests/fixtures/ios27/gestures/ (iOS 27.2).

@main
struct GesturesApp: App {
    var body: some Scene {
        WindowGroup { GesturesView() }
    }
}

struct GesturesView: View {
    @State private var offset = CGSize.zero
    @GestureState private var dragging = CGSize.zero
    @State private var held = false
    @State private var doubleTaps = 0
    @State private var zoom: CGFloat = 1
    @State private var turn = Angle.zero

    var body: some View {
        NavigationStack {
            VStack(spacing: 16) {
                RoundedRectangle(cornerRadius: 16)
                    .fill(Color.blue)
                    .frame(width: 140, height: 72)
                    .overlay(Text("Drag me").foregroundStyle(Color.white))
                    .offset(x: offset.width + dragging.width, y: offset.height + dragging.height)
                    .gesture(
                        DragGesture()
                            .updating($dragging) { value, state, _ in state = value.translation }
                            .onEnded { value in
                                offset.width += value.translation.width
                                offset.height += value.translation.height
                            }
                    )
                Text("Offset \(Int(offset.width.rounded())), \(Int(offset.height.rounded()))")
                Circle()
                    .fill(held ? Color.green : Color.orange)
                    .frame(width: 72, height: 72)
                    .overlay(Text("Hold").foregroundStyle(Color.white))
                    .onLongPressGesture { held.toggle() }
                Text(held ? "Long pressed" : "Not pressed yet")
                RoundedRectangle(cornerRadius: 16)
                    .fill(Color.purple)
                    .frame(width: 140, height: 72)
                    .overlay(Text("Double-tap").foregroundStyle(Color.white))
                    .gesture(TapGesture(count: 2).onEnded { doubleTaps += 1 })
                Text("Double taps: \(doubleTaps)")
                RoundedRectangle(cornerRadius: 16)
                    .fill(Color.red)
                    .frame(width: 200, height: 96)
                    .overlay(Text("Pinch or rotate").foregroundStyle(Color.white))
                    .gesture(
                        MagnifyGesture().onEnded { zoom = $0.magnification }
                            .simultaneously(with: RotateGesture().onEnded { turn = $0.rotation })
                    )
                Text(zoom > 1.05 ? "Zoomed in" : zoom < 0.95 ? "Zoomed out" : "Not zoomed")
                Text(turn.degrees > 5 ? "Turned right" : turn.degrees < -5 ? "Turned left" : "Not turned")
            }
            .navigationTitle("Gestures")
        }
    }
}
