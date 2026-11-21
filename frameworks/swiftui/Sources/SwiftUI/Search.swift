#if !os(WASI)
import Foundation
#endif

public struct SearchFieldPlacement: Sendable {
    let location: String

    public struct NavigationBarDrawerDisplayMode: Sendable {
        let alwaysVisible: Bool
        public static let automatic = Self(alwaysVisible: false)
        public static let always = Self(alwaysVisible: true)
    }

    public static let automatic = Self(location: "drawer")
    public static let navigationBarDrawer = Self.automatic
    public static let toolbar = Self(location: "toolbar")
    public static let toolbarPrincipal = Self(location: "principal")
    public static let sidebar = Self.automatic

    public static func navigationBarDrawer(displayMode: NavigationBarDrawerDisplayMode) -> Self {
        displayMode.alwaysVisible ? Self(location: "always") : .navigationBarDrawer
    }
}

public struct DismissSearchAction {
    var action: () -> Void = {}
    public func callAsFunction() { action() }
}

private struct _IsSearchingKey: EnvironmentKey { static let defaultValue = false }
private struct _DismissSearchKey: EnvironmentKey { static var defaultValue: DismissSearchAction { .init() } }

extension EnvironmentValues {
    public var isSearching: Bool { self[_IsSearchingKey.self] }
    public var dismissSearch: DismissSearchAction { self[_DismissSearchKey.self] }
}

private struct _Searchable<Content: View>: View, _Flattenable {
    var body: Never { fatalError() }
    let content: Content
    let text: Binding<String>
    let presented: Binding<Bool>?
    let placement: SearchFieldPlacement
    let prompt: Text?
    @State private var active = false

    init(content: Content, text: Binding<String>, presented: Binding<Bool>?,
         placement: SearchFieldPlacement, prompt: Text?) {
        self.content = content
        self.text = text
        self.presented = presented
        self.placement = placement
        self.prompt = prompt
    }

    func _children(_ path: String) -> [Node] {
        _active._bind(path + "#searchActive")
        let presentation = presented ?? $active
        let dismiss = DismissSearchAction {
            text.wrappedValue = ""
            presentation.wrappedValue = false
        }
        let rt = _Runtime.shared
        var environment = _localEnv()
        environment[ObjectIdentifier(_IsSearchingKey.self)] = presentation.wrappedValue
        environment[ObjectIdentifier(_DismissSearchKey.self)] = dismiss
        let children = _withEnv(environment) { SwiftUI._children(content, path) }
        guard rt.navigationPreferences != nil || children.first?["kind"] as? String == "navigation" else { return children }

        let saved = rt.textInput
        defer { rt.textInput = saved }
        let identity = _active.storage.slot.map(ObjectIdentifier.init) ?? ObjectIdentifier(_active.storage)
        rt.textInput.focus = [
            _TextFocus(identity: identity, value: true, isFocused: { presentation.wrappedValue },
                       focus: { presentation.wrappedValue = true }, blur: { presentation.wrappedValue = false })
        ]
        let submit = saved.submit.filter { $0.0.contains(.search) }.map(\.1)
        rt.textInput.submit = []
        rt.textInput.submitLabel = .search
        var field = _TextInput(text: text, prompt: prompt ?? Text("Search"), style: .plain,
                               commit: submit.isEmpty ? nil : {
                                   if !text.wrappedValue.isEmpty { submit.forEach { $0() } }
                               })
            .emit(nil, path + "/search")
        field["search_field"] = true
        var search: Node = ["field": field, "placement": placement.location]
        if presentation.wrappedValue {
            search["cancel"] = _eval(Button("Close", systemImage: "xmark") { dismiss() }, path + "/searchCancel")
        }
        return _collectNavigationPreferences(["search": search], from: children)
    }
}

extension View {
    public func searchable(text: Binding<String>, placement: SearchFieldPlacement = .automatic,
                           prompt: Text? = nil) -> some View {
        _Searchable(content: self, text: text, presented: nil, placement: placement, prompt: prompt)
    }

    public func searchable<S: StringProtocol>(text: Binding<String>, placement: SearchFieldPlacement = .automatic,
                                              prompt: S) -> some View {
        searchable(text: text, placement: placement, prompt: Text(String(prompt)))
    }

    public func searchable(text: Binding<String>, isPresented: Binding<Bool>, placement: SearchFieldPlacement = .automatic,
                           prompt: Text? = nil) -> some View {
        _Searchable(content: self, text: text, presented: isPresented, placement: placement, prompt: prompt)
    }

    public func searchable<S: StringProtocol>(text: Binding<String>, isPresented: Binding<Bool>,
                                              placement: SearchFieldPlacement = .automatic, prompt: S) -> some View {
        searchable(text: text, isPresented: isPresented, placement: placement, prompt: Text(String(prompt)))
    }
}
