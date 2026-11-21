#if !os(WASI)
import Foundation
#endif

public struct ToolbarItemPlacement: Sendable {
    let region: String
    public static let automatic = Self(region: "trailing")
    public static let principal = Self(region: "principal")
    public static let navigation = Self(region: "leading")
    public static let primaryAction = Self.automatic
    public static let secondaryAction = Self.automatic
    public static let status = Self(region: "bottom")
    public static let confirmationAction = Self.automatic
    public static let cancellationAction = Self.navigation
    public static let destructiveAction = Self.automatic
    public static let topBarLeading = Self.navigation
    public static let topBarTrailing = Self.automatic
    public static let topBarPinnedTrailing = Self.automatic
    public static let navigationBarLeading = Self.topBarLeading
    public static let navigationBarTrailing = Self.topBarTrailing
    public static let bottomBar = Self(region: "bottom")
}

public protocol ToolbarContent {
    associatedtype Body: ToolbarContent
    @ToolbarContentBuilder var body: Body { get }
}

extension Never: ToolbarContent {}

public protocol CustomizableToolbarContent: ToolbarContent {}

protocol _ToolbarSource {
    func items(_ path: String) -> [Node]
}

func _toolbarItems<T: ToolbarContent>(_ value: T, at path: String) -> [Node] {
    if let source = value as? _ToolbarSource { return source.items(path) }
    return _toolbarItems(value.body, at: path)
}

public struct _ToolbarContentList: CustomizableToolbarContent, _ToolbarSource {
    public var body: Never { return fatalError() }
    let entries: [(String) -> [Node]]
    func items(_ path: String) -> [Node] {
        entries.enumerated().flatMap { index, entry in entry("\(path)/\(index)") }
    }
}

@resultBuilder
public enum ToolbarContentBuilder {
    public static func buildExpression<T: ToolbarContent>(_ value: T) -> _ToolbarContentList {
        .init(entries: [{ _toolbarItems(value, at: $0) }])
    }
    public static func buildBlock(_ values: _ToolbarContentList...) -> _ToolbarContentList {
        buildArray(values)
    }
    public static func buildOptional(_ value: _ToolbarContentList?) -> _ToolbarContentList {
        value ?? .init(entries: [])
    }
    public static func buildEither(first: _ToolbarContentList) -> _ToolbarContentList {
        .init(entries: [{ first.items($0 + "/true") }])
    }
    public static func buildEither(second: _ToolbarContentList) -> _ToolbarContentList {
        .init(entries: [{ second.items($0 + "/false") }])
    }
    public static func buildArray(_ values: [_ToolbarContentList]) -> _ToolbarContentList {
        .init(entries: values.map { value in { value.items($0) } })
    }
    public static func buildLimitedAvailability(_ value: _ToolbarContentList) -> _ToolbarContentList { value }
}

public struct ToolbarItem<ID, Content: View>: CustomizableToolbarContent, _ToolbarSource {
    public var body: Never { return fatalError() }
    let id: ID
    let placement: ToolbarItemPlacement
    let showsByDefault: Bool
    let content: Content

    func items(_ path: String) -> [Node] {
        guard showsByDefault else { return [] }
        let itemPath = (id as? String).map { path + "/" + $0 } ?? path
        return [["region": placement.region, "content": _eval(content, itemPath)]]
    }
}

extension ToolbarItem where ID == Void {
    public init(placement: ToolbarItemPlacement = .automatic, @ViewBuilder content: () -> Content) {
        self.init(id: (), placement: placement, showsByDefault: true, content: content())
    }
}

extension ToolbarItem where ID == String {
    public init(id: String, placement: ToolbarItemPlacement = .automatic,
                showsByDefault: Bool = true, @ViewBuilder content: () -> Content) {
        self.init(id: id, placement: placement, showsByDefault: showsByDefault, content: content())
    }
}

public struct ToolbarItemGroup<Content: View>: ToolbarContent, _ToolbarSource {
    public var body: Never { return fatalError() }
    let placement: ToolbarItemPlacement
    let content: Content

    public init(placement: ToolbarItemPlacement = .automatic, @ViewBuilder content: () -> Content) {
        self.placement = placement
        self.content = content()
    }

    func items(_ path: String) -> [Node] {
        _children(content, path).map { ["region": placement.region, "content": $0] }
    }
}

public struct ToolbarPlacement: Hashable, Sendable {
    let key: String
    public static let automatic = Self(key: "automatic")
    public static let navigationBar = Self(key: "navigationBar")
    public static let bottomBar = Self(key: "bottomBar")
    public static let tabBar = Self(key: "tabBar")
}

public enum ToolbarTitleDisplayMode { case automatic, inline, inlineLarge, large }

func _mergeNavigationPreferences(_ earlier: Node, _ later: Node) -> Node {
    var result = earlier.merging(later) { _, new in new }
    if let first = earlier["items"] as? [Node], let second = later["items"] as? [Node] {
        result["items"] = first + second
    }
    return result
}

func _applyNavigationPreferences(_ preferences: Node, to node: Node) -> Node {
    guard !preferences.isEmpty else { return node }
    var result = node
    var chrome = _mergeNavigationPreferences(result["chrome"] as? Node ?? [:], preferences)
    if let mode = chrome.removeValue(forKey: "displayMode") { result["displayMode"] = mode }
    if !chrome.isEmpty { result["chrome"] = chrome }
    return result
}

func _collectNavigationPreferences(_ preferences: Node, from children: [Node]) -> [Node] {
    if children.count == 1, children[0]["kind"] as? String == "navigation" {
        return [_applyNavigationPreferences(preferences, to: children[0])]
    }
    let rt = _Runtime.shared
    if let earlier = rt.navigationPreferences {
        rt.navigationPreferences = _mergeNavigationPreferences(earlier, preferences)
    }
    return children
}

private struct _NavigationPreference<Content: View>: View, _Flattenable {
    var body: Never { fatalError() }
    let content: Content
    let preferences: (String) -> Node

    func _children(_ path: String) -> [Node] {
        let children = SwiftUI._children(content, path + "/content")
        if _Runtime.shared.navigationPreferences == nil && children.first?["kind"] as? String != "navigation" {
            return children
        }
        return _collectNavigationPreferences(preferences(path + "/toolbar"), from: children)
    }
}

extension View {
    public func toolbar<T: ToolbarContent>(@ToolbarContentBuilder content: () -> T) -> some View {
        let toolbar = content()
        return _NavigationPreference(content: self) { ["items": _toolbarItems(toolbar, at: $0)] }
    }

    @_disfavoredOverload
    public func toolbar<C: View>(@ViewBuilder content: () -> C) -> some View {
        let toolbar = ToolbarItem(content: content)
        return _NavigationPreference(content: self) { ["items": toolbar.items($0)] }
    }

    public func toolbar<T: CustomizableToolbarContent>(id: String, @ToolbarContentBuilder content: () -> T) -> some View {
        let toolbar = content()
        return _NavigationPreference(content: self) { ["items": _toolbarItems(toolbar, at: $0 + "/" + id)] }
    }

    private func barPreferences(_ bars: [ToolbarPlacement], _ prefix: String, _ value: Any) -> some View {
        let placements = bars.isEmpty || bars.contains(.automatic) ? [.navigationBar, .bottomBar] : bars
        return _NavigationPreference(content: self) { _ in
            Dictionary(placements.filter { $0 == .navigationBar || $0 == .bottomBar }
                .map { ($0.key + prefix, value) }, uniquingKeysWith: { _, last in last })
        }
    }

    public func toolbar(_ visibility: Visibility, for bars: ToolbarPlacement...) -> some View {
        barPreferences(bars, "Visibility", String(describing: visibility))
    }

    public func toolbarVisibility(_ visibility: Visibility, for bars: ToolbarPlacement...) -> some View {
        barPreferences(bars, "Visibility", String(describing: visibility))
    }

    public func toolbarBackground<S: ShapeStyle>(_ style: S, for bars: ToolbarPlacement...) -> some View {
        barPreferences(bars, "Background", style._json)
    }

    public func toolbarBackground(_ visibility: Visibility, for bars: ToolbarPlacement...) -> some View {
        barPreferences(bars, "BackgroundVisibility", String(describing: visibility))
    }

    public func toolbarBackgroundVisibility(_ visibility: Visibility, for bars: ToolbarPlacement...) -> some View {
        barPreferences(bars, "BackgroundVisibility", String(describing: visibility))
    }

    public func toolbarTitleDisplayMode(_ mode: ToolbarTitleDisplayMode) -> some View {
        _NavigationPreference(content: self) { _ in
            ["displayMode": mode == .inline || mode == .inlineLarge ? "inline" : "large"]
        }
    }

    public func toolbarTitleMenu<C: View>(@ViewBuilder content: () -> C) -> some View {
        let menu = Menu(content: content) { Image(systemName: "chevron.down.circle") }
        return _NavigationPreference(content: self) { ["titleMenu": _eval(menu, $0)] }
    }
}
