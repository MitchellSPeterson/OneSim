#if !os(WASI)
import Foundation
#endif
// Alerts, confirmation dialogs, sheets and full-screen covers (docs/protocol.md section G). Each modifier
// evaluates its presented content in place (so its @State lives under this view's path while shown) and
// hands it to the runtime, which hoists the layers above the whole tree as a `presentation` node.

public enum Visibility { case automatic, visible, hidden }

public struct PresentationDetent: Hashable {
    enum Kind: Hashable { case medium, large, fraction(Double), height(Double) }
    let kind: Kind
    public static let medium = PresentationDetent(kind: .medium)
    public static let large = PresentationDetent(kind: .large)
    public static func fraction(_ f: CGFloat) -> PresentationDetent { .init(kind: .fraction(Double(f))) }
    public static func height(_ h: CGFloat) -> PresentationDetent { .init(kind: .height(Double(h))) }
    var json: Node {
        switch kind {
        case .medium: return ["kind": "medium"]
        case .large: return ["kind": "large"]
        case .fraction(let v): return ["kind": "fraction", "value": v]
        case .height(let v): return ["kind": "height", "value": v]
        }
    }
}

/// `.alert` and `.confirmationDialog`: buttons in `actions` run their action, then clear the binding.
struct _AlertView<Content: View, A: View, M: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, title: String, isPresented: Binding<Bool>, actions: A, message: M?
    let dialog: Bool, titleVisible: Bool
    func _children(_ path: String) -> [Node] {
        let out = SwiftUI._children(content, path)
        let rt = _Runtime.shared, b = isPresented
        guard rt.pageIsActive, b.wrappedValue else { return out }
        let saved = rt.autoDismiss
        rt.autoDismiss = { b.wrappedValue = false }
        defer { rt.autoDismiss = saved }
        var acts = SwiftUI._children(actions, path + "/alert")
        if dialog {
            if !acts.contains(where: { $0["role"] as? String == "cancel" }) {   // iOS adds a Cancel button
                acts += SwiftUI._children(Button("Cancel", role: .cancel) {}, path + "/alert/cancel")
            }
        } else if acts.isEmpty {
            acts = SwiftUI._children(Button("OK") {}, path + "/alert/ok")   // iOS adds an OK button
        }
        var a: Node = dialog ? ["kind": "dialog", "title": title, "titleVisible": titleVisible, "actions": acts] : ["kind": "alert", "title": title, "actions": acts]
        if dialog {
            // Popover: points at the view it is attached to; tapping outside runs the cancel action (which also clears the binding).
            if let id = out.first?["id"] as? String { a["anchor"] = id }
            if let c = acts.first(where: { $0["role"] as? String == "cancel" }), let id = c["action"] as? UInt64, id != 0 { a["dismiss"] = id }
        }
        if let message {
            let m = _eval(message, path + "/alert/m")
            if m["kind"] as? String != "empty" { a["message"] = m }
        }
        rt.layers.append(a)
        return out
    }
}

/// Keys of the presentation modifiers. Like SwiftUI's preferences they may sit on any view of the presented
/// content (not inside a nested presentation); the presentation lifts them off and never emits them as modifiers.
let _presentationKeys = ["presentationDetents", "presentationDragIndicator", "interactiveDismissDisabled", "presentationBackground",
                         "presentationCornerRadius", "presentationBackgroundInteraction", "presentationContentInteraction",
                         "presentationCompactAdaptation", "presentationSizing", "presentationPlacement"]

/// Removes the presentation modifiers from `node` and its descendants; the outermost (closest to the root) wins.
func _liftPresentation(_ node: inout Node) -> Node {
    var found: Node = [:]
    func walk(_ n: inout Node) {
        if var mods = n["modifiers"] as? [Node] {
            // Modifiers are listed innermost first: on one view the outermost (last) wins.
            for m in mods.reversed() { for k in _presentationKeys where found[k] == nil { if let v = m[k] { found[k] = v } } }
            mods.removeAll { m in _presentationKeys.contains { m[$0] != nil } }
            n["modifiers"] = mods
        }
        for k in ["child", "label", "content", "title"] { if var c = n[k] as? Node { walk(&c); n[k] = c } }
        for k in ["children", "rows"] { if var cs = n[k] as? [Node] { for i in cs.indices { walk(&cs[i]) }; n[k] = cs } }
    }
    // The outermost value wins: visit the root's own modifiers first (walk is pre-order).
    walk(&node)
    return found
}

/// `.sheet` / `.fullScreenCover` / `.popover`. A popover adapts to a sheet on iPhone unless its content asks for
/// `.presentationCompactAdaptation(.popover)` (or `.none`); `.fullScreenCover` / `.sheet` adaptations switch kinds.
struct _SheetView<Content: View, S: View>: View, _Flattenable {
    enum Mode { case sheet, cover, popover(PopoverAttachmentAnchor, Edge?) }
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, isPresented: Binding<Bool>, sheet: S, onDismiss: (() -> Void)?, mode: Mode
    func _children(_ path: String) -> [Node] {
        let out = SwiftUI._children(content, path)
        let rt = _Runtime.shared, b = isPresented
        guard rt.pageIsActive, b.wrappedValue else { return out }
        let spath: String
        switch mode { case .sheet: spath = path + "/sheet"; case .cover: spath = path + "/cover"; case .popover: spath = path + "/popover" }
        // onDismiss runs whenever the presentation stops being evaluated: dismiss(), a swipe, or app code clearing the binding.
        rt.noteLifecycle(spath + "#presented", nil, onDismiss)
        rt.layers.append([:])
        let slot = rt.layers.count - 1   // below anything presented from inside
        let saved = (rt.nav, rt.disabled, rt.buttonStyle, rt.dismissHook, rt.autoDismiss, rt.styles.inList)
        // The sheet is its own screen: not inside the presenting navigation stack or list.
        rt.nav = nil; rt.disabled = false; rt.buttonStyle = "automatic"; rt.autoDismiss = nil; rt.styles.inList = false
        rt.dismissHook = { b.wrappedValue = false }
        defer { (rt.nav, rt.disabled, rt.buttonStyle, rt.dismissHook, rt.autoDismiss, rt.styles.inList) = saved }
        var node = _eval(sheet, spath)
        let p = _liftPresentation(&node)
        var kind: String
        switch mode { case .sheet: kind = "sheet"; case .cover: kind = "cover"; case .popover: kind = "popover" }
        if let a = p["presentationCompactAdaptation"] as? String, a != "automatic" {
            kind = a == "none" ? "popover" : a == "fullScreenCover" ? "cover" : a
        } else if case .popover = mode {
            kind = "sheet"   // iPhone: a popover is a sheet unless asked otherwise
        }
        let dismiss = { rt.registerAction { b.wrappedValue = false } }
        switch kind {
        case "cover":
            rt.layers[slot] = ["kind": "cover", "content": node]
        case "popover":
            var l: Node = ["kind": "popover", "content": node, "dismiss": dismiss(), "arrow": true]
            if let id = out.first?["id"] as? String { l["anchor"] = id }
            if case .popover(let anchor, let edge) = mode {
                switch anchor {
                case .rect(let s): if let r = s.rect { l["attachment"] = ["rect": [Double(r.minX), Double(r.minY), Double(r.width), Double(r.height)]] as Node }
                case .point(let u): l["attachment"] = ["point": [Double(u.x), Double(u.y)]] as Node
                }
                if let edge { l["arrowEdge"] = "\(edge)" }
            }
            if p["interactiveDismissDisabled"] as? Bool == true { l["dismiss"] = nil }
            if let bg = p["presentationBackground"] { l["background"] = bg }
            rt.layers[slot] = l
        default:
            var l: Node = ["kind": "sheet", "content": node, "detents": p["presentationDetents"] ?? [Node](),
                           "dragIndicator": p["presentationDragIndicator"] as? String == "visible"]
            if p["interactiveDismissDisabled"] as? Bool != true { l["dismiss"] = dismiss() }   // swipe-to-dismiss
            for (k, w) in [("presentationBackground", "background"), ("presentationCornerRadius", "cornerRadius"),
                           ("presentationBackgroundInteraction", "backgroundInteraction"), ("presentationContentInteraction", "contentInteraction")] {
                if let v = p[k], !(v is NSNull) { l[w] = v }
            }
            rt.layers[slot] = l
        }
        return out
    }
}

/// Where a popover points: the presenting view's bounds (or a rect in its space), or a unit point of it.
public enum PopoverAttachmentAnchor {
    case rect(Anchor<CGRect>.Source)
    case point(UnitPoint)
}

/// An opaque geometry value. Only its `Source` is available (popover attachment).
public struct Anchor<Value> {
    public struct Source {
        let rect: CGRect?   // nil: the view's bounds
    }
}
extension Anchor.Source where Value == CGRect {
    public static var bounds: Anchor<CGRect>.Source { .init(rect: nil) }
    public static func rect(_ r: CGRect) -> Anchor<CGRect>.Source { .init(rect: r) }
}

public struct PresentationAdaptation: Sendable {
    let name: String
    public static var automatic: PresentationAdaptation { .init(name: "automatic") }
    public static var none: PresentationAdaptation { .init(name: "none") }
    public static var popover: PresentationAdaptation { .init(name: "popover") }
    public static var sheet: PresentationAdaptation { .init(name: "sheet") }
    public static var fullScreenCover: PresentationAdaptation { .init(name: "fullScreenCover") }
}

public struct PresentationBackgroundInteraction: @unchecked Sendable {
    let json: Node
    public static var automatic: PresentationBackgroundInteraction { .init(json: ["kind": "automatic"]) }
    public static var enabled: PresentationBackgroundInteraction { .init(json: ["kind": "enabled"]) }
    public static func enabled(upThrough detent: PresentationDetent) -> PresentationBackgroundInteraction {
        .init(json: ["kind": "enabled", "upThrough": detent.json])
    }
    public static var disabled: PresentationBackgroundInteraction { .init(json: ["kind": "disabled"]) }
}

public struct PresentationContentInteraction: Equatable, Sendable {
    let name: String
    public static var automatic: PresentationContentInteraction { .init(name: "automatic") }
    public static var resizes: PresentationContentInteraction { .init(name: "resizes") }
    public static var scrolls: PresentationContentInteraction { .init(name: "scrolls") }
}

/// iOS 27 placement of a presentation in regular width. Compact width (iPhone) always centers it: no effect.
public struct PresentationPlacement: Hashable, Sendable {
    let name: String
    public static let automatic = PresentationPlacement(name: "automatic")
    public static let leading = PresentationPlacement(name: "leading")
    public static let center = PresentationPlacement(name: "center")
    public static let trailing = PresentationPlacement(name: "trailing")
}

/// Sheet sizing in regular width (iPad, Mac). In compact width (iPhone) every sizing fills the width, as on device.
public protocol PresentationSizing {
    func proposedSize(for root: PresentationSizingRoot, context: PresentationSizingContext) -> ProposedViewSize
}
public struct PresentationSizingRoot {
    public func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize { CGSize(width: proposal.width ?? 0, height: proposal.height ?? 0) }
}
@dynamicMemberLookup
public struct PresentationSizingContext {
    public subscript<T>(dynamicMember keyPath: KeyPath<EnvironmentValues, T>) -> T { EnvironmentValues()[keyPath: keyPath] }
}
public struct AutomaticPresentationSizing: PresentationSizing {
    public func proposedSize(for root: PresentationSizingRoot, context: PresentationSizingContext) -> ProposedViewSize { .unspecified }
}
public struct FittedPresentationSizing: PresentationSizing {
    public func proposedSize(for root: PresentationSizingRoot, context: PresentationSizingContext) -> ProposedViewSize { .unspecified }
}
public struct FormPresentationSizing: PresentationSizing {
    public func proposedSize(for root: PresentationSizingRoot, context: PresentationSizingContext) -> ProposedViewSize { .unspecified }
}
public struct PagePresentationSizing: PresentationSizing {
    public func proposedSize(for root: PresentationSizingRoot, context: PresentationSizingContext) -> ProposedViewSize { .unspecified }
}
extension PresentationSizing where Self == AutomaticPresentationSizing { public static var automatic: Self { .init() } }
extension PresentationSizing where Self == FittedPresentationSizing { public static var fitted: Self { .init() } }
extension PresentationSizing where Self == FormPresentationSizing { public static var form: Self { .init() } }
extension PresentationSizing where Self == PagePresentationSizing { public static var page: Self { .init() } }
extension PresentationSizing {
    public func fitted(horizontal: Bool, vertical: Bool) -> some PresentationSizing { self }
    public func sticky(horizontal: Bool, vertical: Bool) -> some PresentationSizing { self }
}

func _itemBinding<Item>(_ item: Binding<Item?>) -> Binding<Bool> {
    Binding(get: { item.wrappedValue != nil }, set: { if !$0 { item.wrappedValue = nil } })
}

extension View {
    // MARK: alert
    public func alert<A: View, M: View>(_ title: String, isPresented: Binding<Bool>, @ViewBuilder actions: () -> A, @ViewBuilder message: () -> M) -> some View {
        _AlertView(content: self, title: title, isPresented: isPresented, actions: actions(), message: message(), dialog: false, titleVisible: true)
    }
    public func alert<A: View>(_ title: String, isPresented: Binding<Bool>, @ViewBuilder actions: () -> A) -> some View {
        _AlertView<Self, A, EmptyView>(content: self, title: title, isPresented: isPresented, actions: actions(), message: nil, dialog: false, titleVisible: true)
    }
    public func alert<T, A: View, M: View>(_ title: String, isPresented: Binding<Bool>, presenting data: T?, @ViewBuilder actions: (T) -> A, @ViewBuilder message: (T) -> M) -> some View {
        _AlertView(content: self, title: title, isPresented: isPresented, actions: data.map(actions), message: data.map(message), dialog: false, titleVisible: true)
    }
    public func alert<T, A: View>(_ title: String, isPresented: Binding<Bool>, presenting data: T?, @ViewBuilder actions: (T) -> A) -> some View {
        _AlertView<Self, A?, EmptyView>(content: self, title: title, isPresented: isPresented, actions: data.map(actions), message: nil, dialog: false, titleVisible: true)
    }

    // MARK: confirmationDialog
    public func confirmationDialog<A: View, M: View>(_ title: String, isPresented: Binding<Bool>, titleVisibility: Visibility = .automatic, @ViewBuilder actions: () -> A, @ViewBuilder message: () -> M) -> some View {
        _AlertView(content: self, title: title, isPresented: isPresented, actions: actions(), message: message(), dialog: true, titleVisible: titleVisibility == .visible)
    }
    public func confirmationDialog<A: View>(_ title: String, isPresented: Binding<Bool>, titleVisibility: Visibility = .automatic, @ViewBuilder actions: () -> A) -> some View {
        _AlertView<Self, A, EmptyView>(content: self, title: title, isPresented: isPresented, actions: actions(), message: nil, dialog: true, titleVisible: titleVisibility == .visible)
    }
    public func confirmationDialog<T, A: View, M: View>(_ title: String, isPresented: Binding<Bool>, titleVisibility: Visibility = .automatic, presenting data: T?, @ViewBuilder actions: (T) -> A, @ViewBuilder message: (T) -> M) -> some View {
        _AlertView(content: self, title: title, isPresented: isPresented, actions: data.map(actions), message: data.map(message), dialog: true, titleVisible: titleVisibility == .visible)
    }
    public func confirmationDialog<T, A: View>(_ title: String, isPresented: Binding<Bool>, titleVisibility: Visibility = .automatic, presenting data: T?, @ViewBuilder actions: (T) -> A) -> some View {
        _AlertView<Self, A?, EmptyView>(content: self, title: title, isPresented: isPresented, actions: data.map(actions), message: nil, dialog: true, titleVisible: titleVisibility == .visible)
    }

    // MARK: sheet / fullScreenCover
    public func sheet<S: View>(isPresented: Binding<Bool>, onDismiss: (() -> Void)? = nil, @ViewBuilder content: () -> S) -> some View {
        _SheetView(content: self, isPresented: isPresented, sheet: content(), onDismiss: onDismiss, mode: .sheet)
    }
    public func sheet<Item, S: View>(item: Binding<Item?>, onDismiss: (() -> Void)? = nil, @ViewBuilder content: (Item) -> S) -> some View {
        _SheetView(content: self, isPresented: _itemBinding(item), sheet: item.wrappedValue.map(content), onDismiss: onDismiss, mode: .sheet)
    }
    public func fullScreenCover<S: View>(isPresented: Binding<Bool>, onDismiss: (() -> Void)? = nil, @ViewBuilder content: () -> S) -> some View {
        _SheetView(content: self, isPresented: isPresented, sheet: content(), onDismiss: onDismiss, mode: .cover)
    }
    public func fullScreenCover<Item, S: View>(item: Binding<Item?>, onDismiss: (() -> Void)? = nil, @ViewBuilder content: (Item) -> S) -> some View {
        _SheetView(content: self, isPresented: _itemBinding(item), sheet: item.wrappedValue.map(content), onDismiss: onDismiss, mode: .cover)
    }

    // MARK: sheet configuration (read by the enclosing `.sheet` from its content's top node)
    public func presentationDetents(_ detents: Set<PresentationDetent>) -> some View {
        _modified { ["presentationDetents": detents.map { $0.json }] }
    }
    public func presentationDragIndicator(_ visibility: Visibility) -> some View {
        _modified { ["presentationDragIndicator": visibility == .visible ? "visible" : "hidden"] }
    }
    public func interactiveDismissDisabled(_ isDisabled: Bool = true) -> some View {
        _modified { ["interactiveDismissDisabled": isDisabled] }
    }
    public func presentationBackground<S: ShapeStyle>(_ style: S) -> some View {
        _modifiedAt { p in ["presentationBackground": _eval(Rectangle().fill(style), p + "/pbg")] }
    }
    public func presentationBackground<V: View>(alignment: Alignment = .center, @ViewBuilder content: () -> V) -> some View {
        let v = content()
        return _modifiedAt { p in ["presentationBackground": _eval(ZStack(alignment: alignment) { Color.clear; v }, p + "/pbg")] }
    }
    /// nil: the system radius.
    public func presentationCornerRadius(_ cornerRadius: CGFloat?) -> some View {
        _modified { ["presentationCornerRadius": cornerRadius.map { Double($0) } as Any? ?? NSNull()] }
    }
    public func presentationBackgroundInteraction(_ interaction: PresentationBackgroundInteraction) -> some View {
        _modified { ["presentationBackgroundInteraction": interaction.json] }
    }
    public func presentationContentInteraction(_ behavior: PresentationContentInteraction) -> some View {
        _modified { ["presentationContentInteraction": behavior.name] }
    }
    public func presentationCompactAdaptation(_ adaptation: PresentationAdaptation) -> some View {
        _modified { ["presentationCompactAdaptation": adaptation.name] }
    }
    /// iPhone portrait is horizontally compact, so `horizontal` applies.
    public func presentationCompactAdaptation(horizontal: PresentationAdaptation, vertical: PresentationAdaptation) -> some View {
        _modified { ["presentationCompactAdaptation": horizontal.name] }
    }
    public func presentationSizing(_ sizing: some PresentationSizing) -> some View {
        _modified { ["presentationSizing": true] }
    }
    public func presentationPlacement(_ placement: PresentationPlacement) -> some View {
        _modified { ["presentationPlacement": placement.name] }
    }

    // MARK: popover (a sheet on iPhone unless the content sets presentationCompactAdaptation)
    public func popover<P: View>(isPresented: Binding<Bool>, attachmentAnchor: PopoverAttachmentAnchor = .rect(.bounds), arrowEdge: Edge? = nil,
                                 @ViewBuilder content: @escaping () -> P) -> some View {
        _SheetView(content: self, isPresented: isPresented, sheet: content(), onDismiss: nil, mode: .popover(attachmentAnchor, arrowEdge))
    }
    public func popover<Item: Identifiable, P: View>(item: Binding<Item?>, attachmentAnchor: PopoverAttachmentAnchor = .rect(.bounds), arrowEdge: Edge? = nil,
                                                     @ViewBuilder content: @escaping (Item) -> P) -> some View {
        _SheetView(content: self, isPresented: _itemBinding(item), sheet: item.wrappedValue.map(content), onDismiss: nil, mode: .popover(attachmentAnchor, arrowEdge))
    }

    // MARK: contextMenu
    public func contextMenu<M: View>(@ViewBuilder menuItems: () -> M) -> some View {
        let m = menuItems()
        return _ContextMenuView<Self, EmptyView>(content: self, items: { p in SwiftUI._children(m, p) }, preview: nil)
    }
    public func contextMenu<M: View, P: View>(@ViewBuilder menuItems: () -> M, @ViewBuilder preview: () -> P) -> some View {
        let m = menuItems()
        return _ContextMenuView(content: self, items: { p in SwiftUI._children(m, p) }, preview: preview())
    }
    @available(*, deprecated, message: "use contextMenu(menuItems:)")
    public func contextMenu<M: View>(_ contextMenu: ContextMenu<M>?) -> some View {
        _ContextMenuView<Self, EmptyView>(content: self, items: { p in contextMenu.map { SwiftUI._children($0.content, p) } ?? [] }, preview: nil)
    }
}

/// Deprecated holder of `contextMenu(_:)` items.
public struct ContextMenu<MenuItems: View> {
    let content: MenuItems
    public init(@ViewBuilder menuItems: () -> MenuItems) { content = menuItems() }
}

final class _ContextMenuState { var open = false; var selection: AnyHashable? }

/// `.contextMenu`: a long press lifts the view (or shows `preview`) over a blurred screen with the menu next to
/// it; an item runs its action and closes the menu, a tap outside closes it.
struct _ContextMenuView<Content: View, P: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, items: (String) -> [Node], preview: P?
    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared
        let key = "\(path)#ContextMenu"
        rt.touched.insert(key)
        let st = (rt.slots[key] as? _ContextMenuState) ?? _ContextMenuState()
        rt.slots[key] = st
        let press = LongPressGesture(minimumDuration: 0.5).onEnded { _ in st.open = true; rt.dirty = true }
        var out = SwiftUI._children(content.simultaneousGesture(press), path)
        guard st.open, rt.pageIsActive, !rt.disabled else { st.open = false; return out }
        var list: [Node] = [], divider = false
        let nodes = _without({ $0.button = nil }) { items(path + "/menu") }
        _menuItems(nodes, &list, &divider)
        for i in list.indices where list[i]["disabled"] == nil {
            let inner = rt.actions[(list[i]["action"] as? UInt64) ?? 0]
            list[i]["action"] = rt.registerAction { st.open = false; rt.dirty = true; inner?() }
        }
        guard !list.isEmpty else { st.open = false; return out }   // an empty menu does not open
        var l: Node = ["kind": "contextMenu", "items": list, "dismiss": rt.registerAction { st.open = false; rt.dirty = true }]
        if let id = out.first?["id"] as? String { l["anchor"] = id }
        if let preview { l["preview"] = _eval(preview, path + "/preview") }
        rt.layers.append(l)
        if !out.isEmpty { out[0]["value"] = "menu" }
        return out
    }
}

/// `contextMenu(forSelectionType:)`: menus for the rows of the `ForEach`es below whose element ids are `I`.
struct _RowContextMenu {
    let menu: (AnyHashable) -> ((String) -> [Node])?
    let primary: ((AnyHashable) -> Void)?
}

struct _SelectionMenuView<Content: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, menu: _RowContextMenu
    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared, saved = rt.rowContextMenu
        rt.rowContextMenu = menu
        defer { rt.rowContextMenu = saved }
        return SwiftUI._children(content, path)
    }
}

/// One `ForEach` row under `contextMenu(forSelectionType:)`: a long press opens the menu built for `[id]`, a tap
/// runs `primaryAction` with `[id]`. Rows whose id is not of the selection type get neither.
func _selectionRow<V: View>(_ row: V, _ id: AnyHashable, _ path: String) -> [Node] {
    let rt = _Runtime.shared
    guard let cm = rt.rowContextMenu, let items = cm.menu(id) else { return SwiftUI._children(row, path) }
    rt.rowContextMenu = nil   // the menu's own content and nested rows are not selection rows
    defer { rt.rowContextMenu = cm }
    let menu = _ContextMenuView<V, EmptyView>(content: row, items: items, preview: nil)
    guard let primary = cm.primary else { return SwiftUI._children(menu, path) }
    return SwiftUI._children(menu.simultaneousGesture(TapGesture().onEnded { primary(id) }), path)
}

extension View {
    /// Menus for the rows of a `List`'s `ForEach` (by element id): the menu is built for the pressed row;
    /// `primaryAction` runs on a tap of a row. An empty menu does not open.
    public func contextMenu<I: Hashable, M: View>(forSelectionType itemType: I.Type = I.self, @ViewBuilder menu: @escaping (Set<I>) -> M,
                                                  primaryAction: ((Set<I>) -> Void)? = nil) -> some View {
        _SelectionMenuView(content: self, menu: _RowContextMenu(
            menu: { tag in (tag.base as? I).map { t in { p in SwiftUI._children(menu([t]), p) } } },
            primary: primaryAction.map { f in { tag in if let t = tag.base as? I { f([t]) } } }))
    }
}
