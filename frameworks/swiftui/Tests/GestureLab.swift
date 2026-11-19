import SwiftUI

// Wire-test app for gestures (protocol section L): every case shows its state as text, so wire_test.py can drive
// touches at each gesture key and read the result. Not a visual reference.

/// A custom gesture: its body is what gets recognized.
struct DoubleTap: Gesture {
    var body: some Gesture<Void> { TapGesture(count: 2) }
}

struct GestureLab: View {
    @State private var total = CGSize.zero
    @GestureState private var live = CGSize.zero
    @State private var globalAt = CGPoint.zero
    @State private var longs = 0
    @State private var pressing = false
    @State private var doubles = 0
    @State private var tapAt = CGPoint.zero
    @State private var scale: CGFloat = 1
    @State private var angle = 0.0
    @State private var pinchDone = ""
    @State private var seq = "idle"
    @State private var excl = "none"
    @State private var nested = ""
    @State private var custom = 0
    @State private var masked = ""
    @GestureState private var held = false

    var body: some View {
        VStack {
            Text("Drag \(Int(live.width)) total \(Int(total.width)) global \(Int(globalAt.x))")
                .gesture(
                    DragGesture()
                        .updating($live) { v, s, _ in s = v.translation }
                        .onEnded { v in total.width += v.translation.width }
                        .simultaneously(with: DragGesture(minimumDistance: 0, coordinateSpace: .global).onChanged { globalAt = $0.location })
                )
            Text("Long \(longs) pressing \(pressing ? "yes" : "no") held \(held ? "yes" : "no")")
                .onLongPressGesture(minimumDuration: 0.5, perform: { longs += 1 }, onPressingChanged: { pressing = $0 })
                .simultaneousGesture(LongPressGesture(minimumDuration: 2).updating($held) { v, s, _ in s = v })
            Text("Double \(doubles)")
                .onTapGesture(count: 2) { doubles += 1 }
            Text("Tap at \(Int(tapAt.x)),\(Int(tapAt.y))")
                .gesture(SpatialTapGesture().onEnded { tapAt = $0.location })
            Text("Pinch \(Int((scale * 100).rounded())) turn \(Int(angle.rounded())) \(pinchDone)")
                .gesture(
                    MagnifyGesture().onChanged { scale = $0.magnification }
                        .simultaneously(with: RotateGesture().onChanged { angle = $0.rotation.degrees })
                        .onEnded { _ in pinchDone = "done" }
                )
            Text("Seq \(seq)")
                .gesture(
                    LongPressGesture(minimumDuration: 0.3)
                        .sequenced(before: DragGesture(minimumDistance: 0))
                        .onChanged { v in
                            switch v {
                            case .first: seq = "pressing"
                            case .second(_, let d): seq = "dragging \(Int(d?.translation.width ?? 0))"
                            }
                        }
                        .onEnded { _ in seq = "done" }
                )
            Text("Excl \(excl)")
                .gesture(
                    TapGesture(count: 2).map { "double" }
                        .exclusively(before: DragGesture().map { _ in "drag" })
                        .onEnded { v in
                            switch v {
                            case .first(let s), .second(let s): excl = s
                            }
                        }
                )
            Text("Nested \(nested)")
                .gesture(TapGesture().onEnded { nested += "c" })
                .padding()
                .gesture(TapGesture().onEnded { nested += "p" })
                .padding()
                .highPriorityGesture(DragGesture().onEnded { _ in nested += "H" })
                .simultaneousGesture(TapGesture().onEnded { nested += "s" })
            Text("Custom \(custom)")
                .gesture(AnyGesture(DoubleTap().onEnded { custom += 1 }))
            HStack {
                Text("Masked \(masked)")
                    .gesture(TapGesture().onEnded { masked += "c" })
            }
            .gesture(TapGesture().onEnded { masked += "p" }, including: .subviews)
        }
    }
}
