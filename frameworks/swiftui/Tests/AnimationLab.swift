import SwiftUI

// Wire-test app for SwiftUI animation & transitions (protocol section P).

struct SlideFadeTransition: Transition {
    func body(content: Content, phase: TransitionPhase) -> some View {
        content
            .opacity(phase.isIdentity ? 1 : 0)
            .offset(x: phase == .willAppear ? -20 : (phase == .didDisappear ? 20 : 0), y: 0)
    }
}

struct StepCurve: CustomAnimation {
    func animate<V: VectorArithmetic>(value: V, time: TimeInterval, context: inout AnimationContext<V>) -> V? {
        if time >= 0.2 { return nil }
        return value.scaled(by: time / 0.2)
    }
}

struct CustomTxKey: TransactionKey {
    static let defaultValue: Int = 0
}

struct KfVals {
    var x: Double = 0
    var y: Double = 0
}

struct ContentTransitionReader: View {
    @Environment(\.contentTransition) var ct
    var body: some View {
        Text(ct == .numericText() ? "ct:numeric" : "ct:other")
    }
}

struct AnimationLab: View {
    @Namespace private var ns
    @State private var shown = false
    @State private var multi = 0
    @State private var count = 0
    @State private var other = 0
    @State private var completed = 0
    @State private var phaseTrigger = 0
    @State private var kfTrigger = 0
    @State private var txVal = 0
    @State private var onChgSrc = 0
    @State private var onChgDst = 0

    // Base epoch 1_700_000_000 (set via SIMCORE_TIME in wire_test.py).
    let t0 = Date(timeIntervalSince1970: 1_700_000_000)
    let t1 = Date(timeIntervalSince1970: 1_700_000_001)
    let t2 = Date(timeIntervalSince1970: 1_700_000_002)

    var body: some View {
        VStack {
            HStack {
                Button("Toggle") {
                    withAnimation(.linear(duration: 0.2)) {
                        shown.toggle()
                        multi += 1
                    } completion: {
                        completed += 1
                    }
                }
                Button("Inc") { count += 1 }
                Button("Other") { other += 1 }
                Button("Phase") { phaseTrigger += 1 }
                Button("Keyframe") { kfTrigger += 1 }
                Button("Tx") {
                    var tx = Transaction(animation: Animation(StepCurve()))
                    tx[CustomTxKey.self] = 7
                    withTransaction(tx) { txVal = tx[CustomTxKey.self] }
                }
                Button("OnChg") { onChgSrc += 1 }
            }

            Text("done:\(completed) multi:\(multi) tx:\(txVal) other:\(other) oc:\(onChgDst)")
                .onChange(of: onChgSrc) {
                    withAnimation(.linear(duration: 0.25)) {
                        onChgDst += 1
                    }
                }

            if shown {
                Text("Card")
                    .transition(.asymmetric(insertion: .move(edge: .leading).combined(with: .opacity), removal: .scale(scale: 0.5)))
                Text("CustomTr")
                    .transition(SlideFadeTransition())
                Text("Blur down")
                    .transition(BlurReplaceTransition())
                Text("Blur up")
                    .transition(BlurReplaceTransition(configuration: .upUp))
            }

            Text("Count:\(count)")
                .contentTransition(.numericText())
                .animation(.easeInOut(duration: 0.3), value: count)
            ContentTransitionReader()
                .contentTransition(.numericText())

            Text("Suppressed:\(count)")
                .animation(nil, value: count)

            HStack {
                Text("Src")
                    .matchedGeometryEffect(id: "pill", in: ns, properties: .frame, isSource: true)
                Text("Dst")
                    .matchedGeometryEffect(id: "pill", in: ns, properties: .position, isSource: false)
            }

            PhaseAnimator([0, 1, 2], trigger: phaseTrigger) { phase in
                Text("phase:\(phase)")
            } animation: { _ in
                .linear(duration: 0.1)
            }

            KeyframeAnimator(initialValue: KfVals(), trigger: kfTrigger) { v in
                Text("kf:\(Int(v.x.rounded())),\(Int(v.y.rounded()))")
            } keyframes: { _ in
                KeyframeTrack(\.x) {
                    LinearKeyframe(100, duration: 0.2)
                }
                KeyframeTrack(\.y) {
                    MoveKeyframe(10)
                    CubicKeyframe(50, duration: 0.2)
                }
            }

            HStack {
                TimelineView(.explicit([t0, t1, t2])) { ctx in
                    Text("tl:\(Int(ctx.date.timeIntervalSince(t0)))")
                }
                Text("txmod:\(txVal)")
                    .transaction { tx in
                        if tx[CustomTxKey.self] == 7 { tx.disablesAnimations = true }
                    }
                Text("pm")
                    .phaseAnimator([10, 20], trigger: phaseTrigger) { content, p in
                        content.opacity(p == 20 ? 0.5 : 1.0)
                    } animation: { _ in .linear(duration: 0.1) }
                Text("oneshot")
                    .keyframeAnimator(initialValue: 0.0, repeating: false) { content, v in
                        content.offset(x: v, y: 0)
                    } keyframes: { _ in
                        KeyframeTrack {
                            LinearKeyframe(40.0, duration: 0.1)
                        }
                    }
                Text(AnimationLab.scheduleChecks(t0: t0))
            }
        }
    }

    static func scheduleChecks(t0: Date) -> String {
        let futureStart = t0.addingTimeInterval(10)
        let pEntries = PeriodicTimelineSchedule(from: futureStart, by: 5).entries(from: t0, mode: .normal)
        var pIter = pEntries.makeIterator()
        let p0 = pIter.next()!.timeIntervalSince(t0)
        let p1 = pIter.next()!.timeIntervalSince(t0)

        let aPaused = AnimationTimelineSchedule(minimumInterval: 0.1, paused: true).entries(from: t0, mode: .normal)
        var aIter = aPaused.makeIterator()
        let aOk = (aIter.next() != nil) && (aIter.next() == nil)

        let mEntries = EveryMinuteTimelineSchedule().entries(from: t0.addingTimeInterval(25), mode: .normal)
        var mIter = mEntries.makeIterator()
        let m0 = mIter.next()!.timeIntervalSince1970
        let m1 = mIter.next()!.timeIntervalSince1970
        let mOk = (m1 - m0 == 60.0) && (Int(m0) % 60 == 0)

        return (p0 == 10.0 && p1 == 15.0 && aOk && mOk) ? "sched:ok" : "sched:bad:\(p0),\(p1),\(aOk),\(mOk)"
    }
}
