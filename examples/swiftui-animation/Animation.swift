import SwiftUI

// Animation & transitions: @Namespace + matchedGeometryEffect, .transition, .contentTransition,
// PhaseAnimator, KeyframeAnimator, TimelineView, and withAnimation.
// Builds unchanged against Apple's SwiftUI.

@main
struct AnimationApp: App {
    var body: some Scene {
        WindowGroup { AnimationView() }
    }
}

struct AnimationView: View {
    @Namespace private var ns
    @State private var selected = "Alpha"
    @State private var showCard = false
    @State private var count = 0
    @State private var pulse = 0
    @State private var bounce = 0

    private let fixedDate = Date(timeIntervalSince1970: 1_700_000_000)

    var body: some View {
        NavigationStack {
            VStack(spacing: 16) {
                HStack(spacing: 8) {
                    ForEach(["Alpha", "Beta", "Gamma"], id: \.self) { tab in
                        Button {
                            withAnimation(.spring(duration: 0.35, bounce: 0.15)) {
                                selected = tab
                            }
                        } label: {
                            Text(tab)
                                .foregroundStyle(selected == tab ? Color.white : Color.primary)
                                .padding(.horizontal, 16)
                                .padding(.vertical, 8)
                                .background(
                                    Group {
                                        if selected == tab {
                                            Capsule()
                                                .fill(Color.blue)
                                                .matchedGeometryEffect(id: "tab", in: ns)
                                        }
                                    }
                                )
                        }
                        .buttonStyle(.plain)
                    }
                }

                HStack(spacing: 12) {
                    Button("Toggle Card") {
                        withAnimation(.easeInOut(duration: 0.3)) {
                            showCard.toggle()
                        }
                    }
                    .buttonStyle(.borderedProminent)

                    Button("Increment") {
                        withAnimation(.snappy(duration: 0.25)) {
                            count += 1
                        }
                    }
                    .buttonStyle(.bordered)
                }

                if showCard {
                    RoundedRectangle(cornerRadius: 12)
                        .fill(Color.green)
                        .frame(width: 240, height: 56)
                        .overlay(Text("Animated Card").foregroundStyle(Color.white).bold())
                        .transition(.asymmetric(insertion: .move(edge: .bottom).combined(with: .opacity), removal: .opacity))
                }

                Text("Count: \(count)")
                    .font(.title2)
                    .bold()
                    .contentTransition(.numericText())

                PhaseAnimator([false, true], trigger: pulse) { active in
                    RoundedRectangle(cornerRadius: 12)
                        .fill(active ? Color.orange : Color.purple)
                        .frame(width: 180, height: 44)
                        .overlay(Text(active ? "Pulsing" : "Phase Ready").foregroundStyle(Color.white))
                } animation: { _ in
                    .easeInOut(duration: 0.25)
                }

                KeyframeAnimator(initialValue: 0.0, trigger: bounce) { offsetX in
                    RoundedRectangle(cornerRadius: 12)
                        .fill(Color.pink)
                        .frame(width: 180, height: 44)
                        .overlay(Text("Keyframe Ready").foregroundStyle(Color.white))
                        .offset(x: offsetX, y: 0)
                } keyframes: { _ in
                    KeyframeTrack {
                        LinearKeyframe(24.0, duration: 0.15)
                        SpringKeyframe(0.0, duration: 0.25)
                    }
                }

                TimelineView(.explicit([fixedDate])) { ctx in
                    Text("Timeline \(Int(ctx.date.timeIntervalSince(fixedDate)))")
                        .font(.footnote)
                        .foregroundStyle(Color.secondary)
                }
            }
            .padding(16)
            .navigationTitle("Animation")
        }
    }
}
