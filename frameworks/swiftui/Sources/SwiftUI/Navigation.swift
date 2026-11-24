#if !os(WASI)
import Foundation
#endif

public struct NavigationView<Content: View>: View {
    let content: Content
    public init(@ViewBuilder content: () -> Content) { self.content = content() }
    public var body: some View { NavigationStack { content } }
}

public protocol NavigationViewStyle {}
public struct DefaultNavigationViewStyle: NavigationViewStyle { public init() {} }
public struct StackNavigationViewStyle: NavigationViewStyle { public init() {} }
extension NavigationViewStyle where Self == StackNavigationViewStyle {
    public static var stack: Self { .init() }
}

public enum NavigationSplitViewColumn: Hashable, Sendable { case sidebar, content, detail }
public enum NavigationSplitViewVisibility: Hashable, Sendable { case automatic, all, doubleColumn, detailOnly }

/// The compact presentation selects a single column and gives later columns a back action.
public struct NavigationSplitView<Sidebar: View, Content: View, Detail: View>: View, _Primitive {
    public var body: Never { fatalError() }
    let sidebar: Sidebar
    let content: Content
    let detail: Detail
    let preferredColumn: Binding<NavigationSplitViewColumn>?
    let hasContent: Bool
    @State private var compactColumn: NavigationSplitViewColumn = .sidebar

    private init(preferredColumn: Binding<NavigationSplitViewColumn>?, sidebar: Sidebar, content: Content,
                 detail: Detail, hasContent: Bool) {
        self.sidebar = sidebar
        self.content = content
        self.detail = detail
        self.preferredColumn = preferredColumn
        self.hasContent = hasContent
    }

    public init(@ViewBuilder sidebar: () -> Sidebar, @ViewBuilder content: () -> Content, @ViewBuilder detail: () -> Detail) {
        self.init(preferredColumn: nil, sidebar: sidebar(), content: content(), detail: detail(), hasContent: true)
    }

    public init(columnVisibility: Binding<NavigationSplitViewVisibility>,
                @ViewBuilder sidebar: () -> Sidebar, @ViewBuilder content: () -> Content, @ViewBuilder detail: () -> Detail) {
        self.init(sidebar: sidebar, content: content, detail: detail)
    }

    public init(preferredCompactColumn: Binding<NavigationSplitViewColumn>,
                @ViewBuilder sidebar: () -> Sidebar, @ViewBuilder content: () -> Content, @ViewBuilder detail: () -> Detail) {
        self.init(preferredColumn: preferredCompactColumn, sidebar: sidebar(), content: content(), detail: detail(), hasContent: true)
    }

    public init(columnVisibility: Binding<NavigationSplitViewVisibility>, preferredCompactColumn: Binding<NavigationSplitViewColumn>,
                @ViewBuilder sidebar: () -> Sidebar, @ViewBuilder content: () -> Content, @ViewBuilder detail: () -> Detail) {
        self.init(preferredCompactColumn: preferredCompactColumn, sidebar: sidebar, content: content, detail: detail)
    }

    func _emit(_ path: String) -> Node {
        _compactColumn._bind(path + "#compactColumn")
        let binding = preferredColumn ?? $compactColumn
        let column = binding.wrappedValue == .content && !hasContent ? .detail : binding.wrappedValue
        let rt = _Runtime.shared
        for other in [NavigationSplitViewColumn.sidebar, .content, .detail] where other != column {
            let prefix = "\(path)/\(other)"
            for key in rt.slots.keys where key.hasPrefix(prefix + "/") || key.hasPrefix(prefix + "#") {
                rt.touched.insert(key)
            }
        }
        let previous: NavigationSplitViewColumn = column == .detail && hasContent ? .content : .sidebar
        let back: (() -> Void)? = column == .sidebar ? nil : { binding.wrappedValue = previous }
        let previousState = rt.slots[_NavState.slotKey("\(path)/\(previous)")] as? _NavState
        let backLabel = previousState.flatMap { $0.titles[$0.depth] } ?? "Back"
        return _navigation(path, columnPath: "\(path)/\(column)", rootBack: back, backLabel: backLabel) { screen in
            switch column {
            case .sidebar: return _eval(sidebar, screen)
            case .content: return _eval(content, screen)
            case .detail: return _eval(detail, screen)
            }
        }
    }
}

extension NavigationSplitView where Content == EmptyView {
    public init(@ViewBuilder sidebar: () -> Sidebar, @ViewBuilder detail: () -> Detail) {
        self.init(preferredColumn: nil, sidebar: sidebar(), content: EmptyView(), detail: detail(), hasContent: false)
    }

    public init(columnVisibility: Binding<NavigationSplitViewVisibility>,
                @ViewBuilder sidebar: () -> Sidebar, @ViewBuilder detail: () -> Detail) {
        self.init(sidebar: sidebar, detail: detail)
    }

    public init(preferredCompactColumn: Binding<NavigationSplitViewColumn>,
                @ViewBuilder sidebar: () -> Sidebar, @ViewBuilder detail: () -> Detail) {
        self.init(preferredColumn: preferredCompactColumn, sidebar: sidebar(), content: EmptyView(), detail: detail(), hasContent: false)
    }

    public init(columnVisibility: Binding<NavigationSplitViewVisibility>, preferredCompactColumn: Binding<NavigationSplitViewColumn>,
                @ViewBuilder sidebar: () -> Sidebar, @ViewBuilder detail: () -> Detail) {
        self.init(preferredCompactColumn: preferredCompactColumn, sidebar: sidebar, detail: detail)
    }
}

extension View {
    public func navigationViewStyle<S: NavigationViewStyle>(_ style: S) -> some View { self }
}
