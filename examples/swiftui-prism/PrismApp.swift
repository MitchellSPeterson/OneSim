import SwiftUI

// MARK: - Palettes & Models

enum PrismPalette: String, CaseIterable, Identifiable {
    case coral = "Coral"
    case mint = "Mint"
    case berry = "Berry"
    case solar = "Solar"

    var id: String { rawValue }

    var primary: Color {
        switch self {
        case .coral: return Color(red: 1.0, green: 0.369, blue: 0.384)
        case .mint:  return Color(red: 0.0, green: 0.722, blue: 0.580)
        case .berry: return Color(red: 0.557, green: 0.267, blue: 0.678)
        case .solar: return Color(red: 0.953, green: 0.612, blue: 0.071)
        }
    }

    var secondary: Color {
        switch self {
        case .coral: return Color(red: 1.0, green: 0.600, blue: 0.400)
        case .mint:  return Color(red: 0.0, green: 0.808, blue: 0.788)
        case .berry: return Color(red: 0.910, green: 0.263, blue: 0.576)
        case .solar: return Color(red: 0.902, green: 0.494, blue: 0.133)
        }
    }

    var accent: Color {
        switch self {
        case .coral: return Color(red: 1.0, green: 0.820, blue: 0.400)
        case .mint:  return Color(red: 0.333, green: 0.937, blue: 0.769)
        case .berry: return Color(red: 0.992, green: 0.475, blue: 0.659)
        case .solar: return Color(red: 0.945, green: 0.769, blue: 0.059)
        }
    }

    var moodTag: String {
        switch self {
        case .coral: return "Sunset Energy"
        case .mint:  return "Fresh Focus"
        case .berry: return "Electric Flow"
        case .solar: return "Golden Spark"
        }
    }

    var symbol: String {
        switch self {
        case .coral: return "flame.fill"
        case .mint:  return "leaf.fill"
        case .berry: return "sparkles"
        case .solar: return "sun.max.fill"
        }
    }

    var gradient: LinearGradient {
        LinearGradient(
            colors: [primary, secondary, accent],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
        )
    }
}

enum SparkCategory: String, CaseIterable, Identifiable {
    case play = "Play"
    case make = "Make"
    case move = "Move"
    case calm = "Calm"

    var id: String { rawValue }

    var symbol: String {
        switch self {
        case .play: return "wand.and.stars"
        case .make: return "paintbrush.fill"
        case .move: return "figure.walk"
        case .calm: return "wind"
        }
    }

    var color: Color {
        switch self {
        case .play: return Color(red: 1.0, green: 0.369, blue: 0.384)
        case .make: return Color(red: 0.557, green: 0.267, blue: 0.678)
        case .move: return Color(red: 0.0, green: 0.722, blue: 0.580)
        case .calm: return Color(red: 0.953, green: 0.612, blue: 0.071)
        }
    }
}

struct SparkItem: Identifiable, Equatable {
    let id: Int
    var title: String
    var note: String
    var category: SparkCategory
    var isCompleted: Bool
}

enum LabMotionMode: String, CaseIterable, Identifiable {
    case wave = "Wave"
    case orbit = "Orbit"
    case pulse = "Pulse"

    var id: String { rawValue }
}

// MARK: - App Entry

@main
struct PrismApp: App {
    var body: some Scene {
        WindowGroup {
            PrismRootView()
        }
    }
}

struct PrismRootView: View {
    @State private var selectedTab: Int = 0
    @State private var palette: PrismPalette = .coral
    @State private var vibeScore: Int = 42
    @State private var glowMode: Bool = true
    @State private var sparks: [SparkItem] = [
        SparkItem(id: 1, title: "Sketch a cloud", note: "Spend two minutes drawing the softest shape you see outside.", category: .make, isCompleted: true),
        SparkItem(id: 2, title: "Capture a sunset", note: "Notice three warm gradients in the sky before dusk fades.", category: .play, isCompleted: false),
        SparkItem(id: 3, title: "Ship a tiny delight", note: "Polish one micro-interaction until it feels bouncy and joyful.", category: .make, isCompleted: false),
        SparkItem(id: 4, title: "Hydrate & stretch", note: "Drink a tall glass of water and roll your shoulders back.", category: .move, isCompleted: true),
        SparkItem(id: 5, title: "Three slow breaths", note: "Inhale for four counts, hold for two, exhale for six.", category: .calm, isCompleted: false)
    ]
    @State private var nextSparkID: Int = 6

    var body: some View {
        TabView(selection: $selectedTab) {
            PulseTabView(
                palette: $palette,
                vibeScore: $vibeScore,
                glowMode: $glowMode
            )
            .tabItem {
                Label("Pulse", systemImage: "sparkles")
            }
            .tag(0)

            SparksTabView(
                palette: palette,
                sparks: $sparks,
                nextSparkID: $nextSparkID
            )
            .tabItem {
                Label("Sparks", systemImage: "bolt.fill")
            }
            .tag(1)

            LabTabView(
                palette: $palette,
                vibeScore: $vibeScore,
                glowMode: $glowMode
            )
            .tabItem {
                Label("Lab", systemImage: "slider.horizontal.3")
            }
            .tag(2)
        }
        .tint(palette.primary)
    }
}

// MARK: - Tab 1: Pulse

struct PulseTabView: View {
    @Binding var palette: PrismPalette
    @Binding var vibeScore: Int
    @Binding var glowMode: Bool

    @Environment(\.colorScheme) private var colorScheme
    @GestureState private var orbDragOffset: CGSize = .zero
    @State private var orbPulseBoost: Bool = false
    @State private var statusBanner: String = "Drag, tap, or long-press the orb"

    private var energyFraction: Double {
        min(1.0, Double(vibeScore % 100) / 100.0)
    }

    private var cardSurface: Color {
        colorScheme == .dark ? Color(white: 0.14) : Color(white: 0.96)
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 18) {
                    heroCard
                    paletteSwitcher
                    orbPlayground
                    energyControlsCard
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
            }
            .navigationTitle("Prism Pulse")
        }
    }

    private var heroCard: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 28)
                .fill(palette.gradient)
                .shadow(
                    color: glowMode ? palette.primary.opacity(0.45) : Color.black.opacity(0.15),
                    radius: glowMode ? 18 : 8,
                    x: 0,
                    y: 8
                )

            // Floating decorative orbs driven by PhaseAnimator
            PhaseAnimator([0, 1, 2]) { phase in
                ZStack {
                    Circle()
                        .fill(Color.white.opacity(0.22))
                        .frame(width: 96, height: 96)
                        .offset(
                            x: phase == 0 ? -110 : (phase == 1 ? -96 : -118),
                            y: phase == 0 ? -52 : (phase == 1 ? -38 : -60)
                        )
                        .scaleEffect(phase == 1 ? 1.12 : 0.96)

                    Circle()
                        .fill(palette.accent.opacity(0.35))
                        .frame(width: 72, height: 72)
                        .offset(
                            x: phase == 0 ? 116 : (phase == 1 ? 104 : 124),
                            y: phase == 0 ? 44 : (phase == 1 ? 30 : 52)
                        )
                        .scaleEffect(phase == 2 ? 1.15 : 0.95)
                }
            } animation: { _ in
                .easeInOut(duration: 1.8)
            }

            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    HStack(spacing: 6) {
                        Image(systemName: palette.symbol)
                        Text(palette.moodTag)
                            .font(.caption.weight(.semibold))
                    }
                    .foregroundStyle(.white)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .glassEffect(.regular.tint(palette.primary.opacity(0.35)))

                    Spacer()

                    Text(palette.rawValue.uppercased())
                        .font(.caption2.weight(.bold))
                        .foregroundStyle(.white.opacity(0.9))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 5)
                        .background(Color.black.opacity(0.18), in: Capsule())
                }

                HStack(alignment: .bottom, spacing: 8) {
                    Text("\(vibeScore)")
                        .font(.system(size: 54, weight: .bold, design: .rounded))
                        .foregroundStyle(.white)
                        .contentTransition(.numericText())

                    VStack(alignment: .leading, spacing: 2) {
                        Text("VIBE SCORE")
                            .font(.caption.weight(.bold))
                            .foregroundStyle(.white.opacity(0.85))
                        Text(glowMode ? "Glow Active" : "Calm Mode")
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(0.75))
                    }
                    Spacer()
                    Button {
                        triggerBoost(by: 5, label: "+5 Pulse Boost!")
                    } label: {
                        Label("Boost", systemImage: "wand.and.stars")
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.white)
                            .padding(.horizontal, 14)
                            .padding(.vertical, 9)
                    }
                    .glassEffect(.regular.interactive())
                }
            }
            .padding(20)
        }
        .frame(height: 176)
    }

    private var paletteSwitcher: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Palette")
                .font(.headline)

            HStack(spacing: 10) {
                ForEach(PrismPalette.allCases) { item in
                    let isSelected = item == palette
                    Button {
                        withAnimation(.bouncy) {
                            palette = item
                            statusBanner = "Switched to \(item.rawValue) · \(item.moodTag)"
                        }
                    } label: {
                        VStack(spacing: 6) {
                            Circle()
                                .fill(item.gradient)
                                .frame(width: 34, height: 34)
                                .overlay(
                                    Circle()
                                        .stroke(Color.white, lineWidth: isSelected ? 3 : 0)
                                )
                                .shadow(color: item.primary.opacity(isSelected ? 0.5 : 0.15), radius: isSelected ? 8 : 3)
                            Text(item.rawValue)
                                .font(.caption.weight(isSelected ? .bold : .medium))
                                .foregroundStyle(isSelected ? item.primary : .secondary)
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 10)
                        .background(cardSurface, in: RoundedRectangle(cornerRadius: 16))
                        .overlay(
                            RoundedRectangle(cornerRadius: 16)
                                .stroke(isSelected ? item.primary : Color.clear, lineWidth: 2)
                        )
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    private var orbPlayground: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Vibe Orb Playground")
                    .font(.headline)
                Spacer()
                Text(statusBanner)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            ZStack {
                RoundedRectangle(cornerRadius: 24)
                    .fill(cardSurface)
                    .frame(height: 180)

                Circle()
                    .fill(
                        RadialGradient(
                            colors: [palette.accent, palette.primary, palette.secondary],
                            center: .center,
                            startRadius: 8,
                            endRadius: 56
                        )
                    )
                    .frame(width: 96, height: 96)
                    .overlay(
                        Image(systemName: "sparkles")
                            .font(.title.weight(.bold))
                            .foregroundStyle(.white)
                    )
                    .shadow(
                        color: palette.primary.opacity(glowMode ? 0.65 : 0.25),
                        radius: glowMode ? 20 : 6
                    )
                    .scaleEffect(orbPulseBoost ? 1.18 : (orbDragOffset == .zero ? 1.0 : 1.08))
                    .offset(orbDragOffset)
                    .gesture(
                        DragGesture()
                            .updating($orbDragOffset) { value, state, _ in
                                state = value.translation
                            }
                            .onEnded { _ in
                                triggerBoost(by: 3, label: "+3 Spring Snap!")
                            }
                    )
                    .onTapGesture(count: 2) {
                        triggerBoost(by: 25, label: "+25 Double-Tap Surge!")
                    }
                    .onTapGesture(count: 1) {
                        triggerBoost(by: 5, label: "+5 Tap Pulse!")
                    }
                    .onLongPressGesture(minimumDuration: 0.45) {
                        triggerBoost(by: 50, label: "+50 Supercharge!")
                    }
                    .animation(.bouncy, value: orbDragOffset)
                    .animation(.bouncy, value: orbPulseBoost)
            }
        }
    }

    private var energyControlsCard: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text("Energy Meter")
                    .font(.headline)
                Spacer()
                Text("\(Int(energyFraction * 100))% to next level")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(palette.primary)
            }

            Gauge(value: energyFraction, in: 0...1) {
                Text("Studio Energy")
            } currentValueLabel: {
                Text("\(vibeScore)")
            } minimumValueLabel: {
                Text("0")
            } maximumValueLabel: {
                Text("100")
            }
            .tint(palette.primary)

            ProgressView(value: energyFraction)
                .tint(palette.secondary)

            Toggle(isOn: $glowMode) {
                Label("Glow Mode", systemImage: "sun.max.fill")
                    .font(.subheadline.weight(.medium))
            }
            .tint(palette.primary)
        }
        .padding(16)
        .background(cardSurface, in: RoundedRectangle(cornerRadius: 20))
    }

    private func triggerBoost(by amount: Int, label: String) {
        withAnimation(.bouncy) {
            vibeScore += amount
            statusBanner = label
            orbPulseBoost = true
        }
        withAnimation(.bouncy.delay(0.18)) {
            orbPulseBoost = false
        }
    }
}

// MARK: - Tab 2: Sparks

struct SparksTabView: View {
    let palette: PrismPalette
    @Binding var sparks: [SparkItem]
    @Binding var nextSparkID: Int

    @Environment(\.colorScheme) private var colorScheme
    @State private var showCreateSheet: Bool = false
    @State private var showClearAlert: Bool = false

    private var completedCount: Int {
        sparks.filter { $0.isCompleted }.count
    }

    private var progress: Double {
        sparks.isEmpty ? 0 : Double(completedCount) / Double(sparks.count)
    }

    private var cardSurface: Color {
        colorScheme == .dark ? Color(white: 0.14) : Color(white: 0.96)
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 16) {
                    progressHeader

                    VStack(spacing: 12) {
                        ForEach(sparks) { spark in
                            sparkRow(spark)
                        }
                    }

                    if completedCount > 0 {
                        Button(role: .destructive) {
                            showClearAlert = true
                        } label: {
                            Label("Clear Completed (\(completedCount))", systemImage: "trash")
                                .font(.subheadline.weight(.semibold))
                                .frame(maxWidth: .infinity)
                                .padding(.vertical, 12)
                        }
                        .buttonStyle(.bordered)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
            }
            .navigationTitle("Sparks")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        showCreateSheet = true
                    } label: {
                        Image(systemName: "plus.circle.fill")
                            .font(.title3)
                    }
                }
            }
            .sheet(isPresented: $showCreateSheet) {
                CreateSparkSheet(palette: palette) { title, note, category in
                    withAnimation(.bouncy) {
                        sparks.insert(
                            SparkItem(
                                id: nextSparkID,
                                title: title,
                                note: note,
                                category: category,
                                isCompleted: false
                            ),
                            at: 0
                        )
                        nextSparkID += 1
                    }
                }
                .presentationDetents([.medium])
            }
            .alert("Clear Completed Sparks?", isPresented: $showClearAlert) {
                Button("Cancel", role: .cancel) {}
                Button("Clear", role: .destructive) {
                    withAnimation(.bouncy) {
                        sparks.removeAll { $0.isCompleted }
                    }
                }
            } message: {
                Text("This removes \(completedCount) completed micro-quests from your list.")
            }
        }
    }

    private var progressHeader: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Daily Creative Quests")
                        .font(.headline)
                        .foregroundStyle(.white)
                    Text("\(completedCount) of \(sparks.count) completed")
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(0.85))
                }
                Spacer()
                Text("\(Int(progress * 100))%")
                    .font(.title2.weight(.bold))
                    .foregroundStyle(.white)
                    .contentTransition(.numericText())
            }

            ProgressView(value: progress)
                .tint(.white)
        }
        .padding(18)
        .background(palette.gradient, in: RoundedRectangle(cornerRadius: 22))
    }

    private func sparkRow(_ spark: SparkItem) -> some View {
        HStack(spacing: 12) {
            Button {
                toggleSpark(spark.id)
            } label: {
                Image(systemName: spark.isCompleted ? "checkmark.circle.fill" : "circle")
                    .font(.title2)
                    .foregroundStyle(spark.isCompleted ? palette.primary : .secondary)
            }
            .buttonStyle(.plain)

            NavigationLink {
                SparkDetailView(
                    spark: spark,
                    palette: palette,
                    onToggle: { toggleSpark(spark.id) }
                )
            } label: {
                HStack(spacing: 10) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(spark.title)
                            .font(.body.weight(.semibold))
                            .foregroundStyle(spark.isCompleted ? .secondary : .primary)
                        HStack(spacing: 6) {
                            Label(spark.category.rawValue, systemImage: spark.category.symbol)
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(.white)
                                .padding(.horizontal, 8)
                                .padding(.vertical, 3)
                                .background(spark.category.color, in: Capsule())
                            Text(spark.note)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }
                    Spacer()
                    Image(systemName: "chevron.right")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                }
            }
            .buttonStyle(.plain)
        }
        .padding(14)
        .background(cardSurface, in: RoundedRectangle(cornerRadius: 18))
    }

    private func toggleSpark(_ id: Int) {
        if let index = sparks.firstIndex(where: { $0.id == id }) {
            withAnimation(.bouncy) {
                sparks[index].isCompleted.toggle()
            }
        }
    }
}

struct SparkDetailView: View {
    let spark: SparkItem
    let palette: PrismPalette
    let onToggle: () -> Void

    var body: some View {
        ScrollView {
            VStack(spacing: 20) {
                VStack(spacing: 12) {
                    Image(systemName: spark.category.symbol)
                        .font(.system(size: 44, weight: .bold))
                        .foregroundStyle(.white)
                        .padding(20)
                        .glassEffect(.regular, in: Circle())

                    Text(spark.title)
                        .font(.title2.weight(.bold))
                        .foregroundStyle(.white)

                    Text(spark.category.rawValue.uppercased())
                        .font(.caption.weight(.bold))
                        .foregroundStyle(.white.opacity(0.9))
                        .padding(.horizontal, 12)
                        .padding(.vertical, 5)
                        .background(Color.black.opacity(0.2), in: Capsule())
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 28)
                .background(palette.gradient, in: RoundedRectangle(cornerRadius: 26))

                VStack(alignment: .leading, spacing: 8) {
                    Text("Prompt")
                        .font(.headline)
                    Text(spark.note)
                        .font(.body)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(16)
                .background(Color.gray.opacity(0.12), in: RoundedRectangle(cornerRadius: 18))

                Button {
                    onToggle()
                } label: {
                    Label(
                        spark.isCompleted ? "Mark as Incomplete" : "Complete Spark",
                        systemImage: spark.isCompleted ? "arrow.uturn.backward" : "checkmark.circle.fill"
                    )
                    .font(.headline)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 12)
                }
                .buttonStyle(.borderedProminent)
                .tint(palette.primary)
            }
            .padding(16)
        }
        .navigationTitle("Spark Detail")
    }
}

struct CreateSparkSheet: View {
    let palette: PrismPalette
    let onAdd: (String, String, SparkCategory) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var title: String = ""
    @State private var note: String = ""
    @State private var category: SparkCategory = .play

    var body: some View {
        NavigationStack {
            VStack(spacing: 16) {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Title")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    TextField("e.g. Doodle a tiny rocket", text: $title)
                        .textFieldStyle(.roundedBorder)
                }

                VStack(alignment: .leading, spacing: 6) {
                    Text("Note")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    TextField("Optional creative cue", text: $note)
                        .textFieldStyle(.roundedBorder)
                }

                VStack(alignment: .leading, spacing: 6) {
                    Text("Category")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    Picker("Category", selection: $category) {
                        ForEach(SparkCategory.allCases) { item in
                            Text(item.rawValue).tag(item)
                        }
                    }
                    .pickerStyle(.segmented)
                }

                Spacer()

                Button {
                    let trimmed = trimWhitespace(title)
                    let finalTitle = trimmed.isEmpty ? "New Creative Spark" : trimmed
                    let trimmedNote = trimWhitespace(note)
                    let finalNote = trimmedNote.isEmpty ? "Take five mindful minutes to enjoy this spark." : trimmedNote
                    onAdd(finalTitle, finalNote, category)
                    dismiss()
                } label: {
                    Label("Add Spark", systemImage: "sparkles")
                        .font(.headline)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 12)
                }
                .buttonStyle(.borderedProminent)
                .tint(palette.primary)
            }
            .padding(20)
            .navigationTitle("New Spark")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
    }
}

private func trimWhitespace(_ s: String) -> String {
    var start = s.startIndex
    var end = s.endIndex
    while start < end && (s[start] == " " || s[start] == "\n" || s[start] == "\t" || s[start] == "\r") {
        start = s.index(after: start)
    }
    while end > start {
        let prev = s.index(before: end)
        if s[prev] == " " || s[prev] == "\n" || s[prev] == "\t" || s[prev] == "\r" {
            end = prev
        } else {
            break
        }
    }
    return String(s[start..<end])
}

// MARK: - Tab 3: Lab

struct LabTabView: View {
    @Binding var palette: PrismPalette
    @Binding var vibeScore: Int
    @Binding var glowMode: Bool

    @Environment(\.colorScheme) private var colorScheme
    @State private var cornerRadius: Double = 26
    @State private var rotationDegrees: Double = 12
    @State private var orbCount: Int = 3
    @State private var liquidGlassMode: Bool = true
    @State private var motionMode: LabMotionMode = .wave
    @State private var showFocusMode: Bool = false
    @State private var showQuickActions: Bool = false

    private var cardSurface: Color {
        colorScheme == .dark ? Color(white: 0.14) : Color(white: 0.96)
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 18) {
                    livePreviewCard
                    controlsCard
                    actionsCard
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
            }
            .navigationTitle("Prism Lab")
            .fullScreenCover(isPresented: $showFocusMode) {
                FocusModeCover(palette: palette)
            }
            .confirmationDialog("Lab Quick Actions", isPresented: $showQuickActions) {
                Button("Reset Lab Controls") {
                    withAnimation(.bouncy) {
                        cornerRadius = 26
                        rotationDegrees = 12
                        orbCount = 3
                        liquidGlassMode = true
                        motionMode = .wave
                    }
                }
                Button("Randomize Palette") {
                    let all = PrismPalette.allCases
                    if let idx = all.firstIndex(of: palette) {
                        withAnimation(.bouncy) {
                            palette = all[(idx + 1) % all.count]
                        }
                    }
                }
                Button("Boost Vibe +20") {
                    withAnimation(.bouncy) {
                        vibeScore += 20
                    }
                }
                Button("Cancel", role: .cancel) {}
            }
        }
    }

    private var livePreviewCard: some View {
        ZStack {
            RoundedRectangle(cornerRadius: cornerRadius)
                .fill(palette.gradient)
                .shadow(
                    color: palette.primary.opacity(glowMode ? 0.45 : 0.18),
                    radius: glowMode ? 18 : 6,
                    x: 0,
                    y: 8
                )

            PhaseAnimator([0, 1, 2]) { phase in
                HStack(spacing: 12) {
                    ForEach(0..<orbCount, id: \.self) { idx in
                        let yOffset: CGFloat = {
                            switch motionMode {
                            case .wave:
                                return ((idx + phase) % 2 == 0) ? -14 : 14
                            case .orbit:
                                return phase == 0 ? -10 : (phase == 1 ? 12 : 0)
                            case .pulse:
                                return 0
                            }
                        }()
                        let scale: CGFloat = motionMode == .pulse
                            ? (phase == 1 ? 1.22 : 0.88)
                            : 1.0

                        Circle()
                            .fill(Color.white.opacity(0.32))
                            .frame(width: 34, height: 34)
                            .overlay(
                                Circle()
                                    .stroke(Color.white.opacity(0.75), lineWidth: 1.5)
                            )
                            .scaleEffect(scale)
                            .offset(y: yOffset)
                            .rotationEffect(.degrees(rotationDegrees))
                    }
                }
            } animation: { _ in
                .easeInOut(duration: 1.1)
            }

            VStack {
                HStack {
                    if liquidGlassMode {
                        Text(motionMode.rawValue)
                            .font(.caption.weight(.bold))
                            .foregroundStyle(.white)
                            .padding(.horizontal, 10)
                            .padding(.vertical, 5)
                            .glassEffect(.regular.tint(palette.primary.opacity(0.35)))
                    } else {
                        Text(motionMode.rawValue)
                            .font(.caption.weight(.bold))
                            .foregroundStyle(.white)
                            .padding(.horizontal, 10)
                            .padding(.vertical, 5)
                            .background(Color.black.opacity(0.25), in: Capsule())
                    }
                    Spacer()
                    Text("\(orbCount) Orbs")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.white.opacity(0.9))
                }
                Spacer()
                HStack {
                    Text("Radius \(Int(cornerRadius))pt")
                        .font(.caption2.weight(.medium))
                        .foregroundStyle(.white.opacity(0.85))
                    Spacer()
                    Text("Tilt \(Int(rotationDegrees))°")
                        .font(.caption2.weight(.medium))
                        .foregroundStyle(.white.opacity(0.85))
                }
            }
            .padding(16)
        }
        .frame(height: 170)
    }

    private var controlsCard: some View {
        VStack(alignment: .leading, spacing: 14) {
            Picker("Motion", selection: $motionMode) {
                ForEach(LabMotionMode.allCases) { mode in
                    Text(mode.rawValue).tag(mode)
                }
            }
            .pickerStyle(.segmented)

            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Text("Corner Radius")
                        .font(.subheadline.weight(.medium))
                    Spacer()
                    Text("\(Int(cornerRadius)) pt")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Slider(value: $cornerRadius, in: 8...44)
                    .tint(palette.primary)
            }

            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Text("Rotation & Glow")
                        .font(.subheadline.weight(.medium))
                    Spacer()
                    Text("\(Int(rotationDegrees))°")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Slider(value: $rotationDegrees, in: -45...45)
                    .tint(palette.secondary)
            }

            Stepper("Orb Count: \(orbCount)", value: $orbCount, in: 1...6)
                .font(.subheadline.weight(.medium))

            Toggle(isOn: $liquidGlassMode) {
                Label("Liquid Glass Badge", systemImage: "drop.halffull")
                    .font(.subheadline.weight(.medium))
            }
            .tint(palette.primary)
        }
        .padding(16)
        .background(cardSurface, in: RoundedRectangle(cornerRadius: 20))
    }

    private var actionsCard: some View {
        VStack(spacing: 12) {
            NavigationLink {
                EffectsAndSymbolsView(palette: palette)
            } label: {
                HStack {
                    Label("Effects & Symbols", systemImage: "paintpalette.fill")
                        .font(.subheadline.weight(.semibold))
                    Spacer()
                    Image(systemName: "chevron.right")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                }
                .padding(14)
                .background(cardSurface, in: RoundedRectangle(cornerRadius: 16))
            }
            .buttonStyle(.plain)

            HStack(spacing: 12) {
                Button {
                    showFocusMode = true
                } label: {
                    Label("Focus Mode", systemImage: "scope")
                        .font(.subheadline.weight(.semibold))
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 10)
                }
                .buttonStyle(.borderedProminent)
                .tint(palette.primary)

                Button {
                    showQuickActions = true
                } label: {
                    Label("Quick Actions", systemImage: "ellipsis.circle")
                        .font(.subheadline.weight(.semibold))
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 10)
                }
                .buttonStyle(.bordered)
            }
        }
    }
}

// MARK: - Effects & Symbols Detail

struct EffectsAndSymbolsView: View {
    let palette: PrismPalette

    private let symbols: [String] = [
        "sparkles", "flame.fill", "wand.and.stars", "sun.max.fill",
        "moon.stars.fill", "bolt.fill", "heart.fill", "leaf.fill",
        "paintpalette.fill", "music.note", "star.fill", "cloud.sun.fill"
    ]

    private let gridColumns: [GridItem] = [
        GridItem(.flexible()),
        GridItem(.flexible()),
        GridItem(.flexible()),
        GridItem(.flexible())
    ]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text("Gradients & Glass")
                    .font(.headline)

                HStack(spacing: 12) {
                    RoundedRectangle(cornerRadius: 18)
                        .fill(
                            AngularGradient(
                                colors: [palette.primary, palette.secondary, palette.accent, palette.primary],
                                center: .center
                            )
                        )
                        .frame(height: 100)
                        .overlay(
                            Text("Angular")
                                .font(.caption.weight(.bold))
                                .foregroundStyle(.white)
                                .padding(.horizontal, 10)
                                .padding(.vertical, 4)
                                .glassEffect(.regular)
                        )

                    RoundedRectangle(cornerRadius: 18)
                        .fill(
                            RadialGradient(
                                colors: [palette.accent, palette.primary, palette.secondary],
                                center: .center,
                                startRadius: 6,
                                endRadius: 72
                            )
                        )
                        .frame(height: 100)
                        .overlay(
                            Text("Radial")
                                .font(.caption.weight(.bold))
                                .foregroundStyle(.white)
                                .padding(.horizontal, 10)
                                .padding(.vertical, 4)
                                .glassEffect(.regular)
                        )
                }

                MeshGradient(
                    width: 3,
                    height: 3,
                    locations: .points([
                        SIMD2<Float>(0.0, 0.0), SIMD2<Float>(0.5, 0.0), SIMD2<Float>(1.0, 0.0),
                        SIMD2<Float>(0.0, 0.5), SIMD2<Float>(0.5, 0.5), SIMD2<Float>(1.0, 0.5),
                        SIMD2<Float>(0.0, 1.0), SIMD2<Float>(0.5, 1.0), SIMD2<Float>(1.0, 1.0)
                    ]),
                    colors: .colors([
                        palette.primary, palette.secondary, palette.accent,
                        palette.secondary, palette.accent, palette.primary,
                        palette.accent, palette.primary, palette.secondary
                    ])
                )
                .frame(height: 120)
                .clipShape(RoundedRectangle(cornerRadius: 20))
                .overlay(
                    Text("MeshGradient + Liquid Glass")
                        .font(.caption.weight(.bold))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                        .glassEffect(.regular.tint(palette.primary.opacity(0.3)))
                )

                Text("SF Symbols")
                    .font(.headline)

                LazyVGrid(columns: gridColumns, spacing: 12) {
                    ForEach(symbols, id: \.self) { name in
                        VStack(spacing: 6) {
                            Image(systemName: name)
                                .font(.title2)
                                .foregroundStyle(palette.primary)
                            Text(name.split(separator: ".").first.map(String.init) ?? name)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 12)
                        .background(Color.gray.opacity(0.12), in: RoundedRectangle(cornerRadius: 14))
                    }
                }
            }
            .padding(16)
        }
        .navigationTitle("Effects & Symbols")
    }
}

// MARK: - Focus Mode Full-Screen Cover

struct FocusModeCover: View {
    let palette: PrismPalette
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        ZStack {
            palette.gradient
                .frame(maxWidth: .infinity, maxHeight: .infinity)

            VStack(spacing: 28) {
                HStack {
                    Spacer()
                    Button {
                        dismiss()
                    } label: {
                        Label("Close", systemImage: "xmark")
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.white)
                            .padding(.horizontal, 14)
                            .padding(.vertical, 8)
                    }
                    .glassEffect(.regular.interactive())
                }
                .padding(.horizontal, 20)
                .padding(.top, 12)

                Spacer()

                PhaseAnimator([0, 1]) { phase in
                    ZStack {
                        Circle()
                            .fill(Color.white.opacity(0.18))
                            .frame(width: 220, height: 220)
                            .scaleEffect(phase == 1 ? 1.18 : 0.86)

                        Circle()
                            .fill(
                                RadialGradient(
                                    colors: [.white.opacity(0.9), palette.accent, palette.primary],
                                    center: .center,
                                    startRadius: 10,
                                    endRadius: 90
                                )
                            )
                            .frame(width: 150, height: 150)
                            .scaleEffect(phase == 1 ? 1.12 : 0.9)
                            .shadow(color: Color.white.opacity(0.45), radius: 24)

                        Text(phase == 1 ? "Exhale" : "Inhale")
                            .font(.title2.weight(.bold))
                            .foregroundStyle(.white)
                    }
                } animation: { _ in
                    .easeInOut(duration: 2.4)
                }

                VStack(spacing: 6) {
                    Text("Prism Focus Mode")
                        .font(.title3.weight(.bold))
                        .foregroundStyle(.white)
                    Text(palette.moodTag)
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(0.85))
                }

                Spacer()
            }
        }
    }
}
