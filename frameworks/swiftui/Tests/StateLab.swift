import SwiftUI
import Foundation

// Wire-test app for state and environment (issue #47): custom EnvironmentKey + .environment, isEnabled, size classes.

struct AccentKey: EnvironmentKey { static let defaultValue = "plain" }
extension EnvironmentValues {
    var accent: String {
        get { self[AccentKey.self] }
        set { self[AccentKey.self] = newValue }
    }
}

struct AccentLabel: View {
    @Environment(\.accent) var accent
    @Environment(\.isEnabled) var enabled
    var body: some View { Text("Accent: \(accent) enabled: \(enabled)") }
}

@Observable final class Profile {
    var name = "ada"
    var likes = 0
}

struct ProfileEditor: View {
    @Bindable var profile: Profile
    var body: some View {
        TextField("Name", text: $profile.name)
        Button("Like") { profile.likes += 1 }   // a plain mutation (no Binding) must re-render
    }
}

struct ProfileBadge: View {
    @Environment(Profile.self) var profile
    @Environment(Profile.self) var maybe: Profile?
    var body: some View { Text("Badge: \(profile.name) \(profile.likes) \(maybe == nil ? "none" : "some")") }
}

struct Draft: View {
    @SceneStorage("draft") var draft = "none"   // one value per key for the whole scene
    var body: some View { Button("Draft: \(draft)") { draft = "saved" } }
}

struct Metrics: View {
    @ScaledMetric var pad = 10.0
    @ScaledMetric(relativeTo: .caption) var cap: CGFloat = 12
    @Environment(\.scenePhase) var phase
    var body: some View { Text("Metric: \(pad) \(Double(cap)) \(phase == .active ? "active" : "other")") }
}

struct LocalLink: View {
    @Environment(\.openURL) var openURL
    @Binding var result: String
    var body: some View { Button("Local") { openURL(URL(string: "app://inside")!) { result = "\($0)" } } }
}

struct Links: View {
    @Environment(\.openURL) var openURL
    @State var result = "-"
    var body: some View {
        Button("Open") { openURL(URL(string: "https://example.com/a")!) }
        LocalLink(result: $result)
            .environment(\.openURL, OpenURLAction { url in url.scheme == "app" ? .handled : .systemAction })
        Text("Result: \(result)")
    }
}

struct ToneKey: EnvironmentKey { static let defaultValue = "none" }
extension EnvironmentValues {
    var tone: String {
        get { self[ToneKey.self] }
        set { self[ToneKey.self] = newValue }
    }
}

struct PushedTheme: View {
    @Environment(\.accent) var accent
    @Environment(\.tone) var tone
    let bump: () -> Void
    var body: some View { Button("Pushed: \(accent) \(tone)") { bump() } }
}

// A pushed screen sees later changes to values set above the stack, and keeps the ones set between stack and link.
struct NavEnv: View {
    @State var theme = "day"
    var body: some View {
        NavigationStack {
            NavigationLink("Push", destination: { PushedTheme { theme = "night" } })
                .environment(\.tone, "inner")
        }
        .environment(\.accent, theme)
    }
}

struct StateLab: View {
    @State var profile = Profile()
    @Environment(\.horizontalSizeClass) var h
    @Environment(\.verticalSizeClass) var v
    var body: some View {
        VStack {
            AccentLabel()
            AccentLabel().environment(\.accent, "loud")
            VStack {
                AccentLabel()
                AccentLabel().environment(\.accent, "inner")
                Button("Inner") {}.environment(\.isEnabled, true)   // still disabled: the ancestor wins
            }
            .environment(\.accent, "outer")
            .disabled(true)
            ProfileEditor(profile: profile)
            ProfileBadge().environment(profile)
            Draft()
            Draft()
            Group {
                Metrics()
                Metrics().environment(\.dynamicTypeSize, .large)
                NavEnv()
            }
            Links()
            Text("Size: \(h == .compact ? "compact" : "regular")/\(v == .compact ? "compact" : "regular")")
        }
    }
}
