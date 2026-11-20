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
        guard isPresented.wrappedValue else { return out }
        let rt = _Runtime.shared, b = isPresented
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

/// `.sheet` / `.fullScreenCover`.
struct _SheetView<Content: View, S: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, isPresented: Binding<Bool>, sheet: S, onDismiss: (() -> Void)?, cover: Bool
    func _children(_ path: String) -> [Node] {
        let out = SwiftUI._children(content, path)
        guard isPresented.wrappedValue else { return out }
        let rt = _Runtime.shared, b = isPresented
        let spath = path + (cover ? "/cover" : "/sheet")
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
        var detents: [Node] = [], indicator = false, locked = false
        let mods = (node["modifiers"] as? [Node]) ?? []
        for m in mods {
            if let d = m["presentationDetents"] as? [Node] { detents = d }
            if let v = m["presentationDragIndicator"] as? String { indicator = v == "visible" }
            if let l = m["interactiveDismissDisabled"] as? Bool { locked = l }
        }
        node["modifiers"] = mods.filter { $0["presentationDetents"] == nil && $0["presentationDragIndicator"] == nil && $0["interactiveDismissDisabled"] == nil }
        if cover {
            rt.layers[slot] = ["kind": "cover", "content": node]
        } else {
            var l: Node = ["kind": "sheet", "content": node, "detents": detents, "dragIndicator": indicator]
            if !locked { l["dismiss"] = rt.registerAction { b.wrappedValue = false } }   // swipe-to-dismiss
            rt.layers[slot] = l
        }
        return out
    }
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
        _SheetView(content: self, isPresented: isPresented, sheet: content(), onDismiss: onDismiss, cover: false)
    }
    public func sheet<Item, S: View>(item: Binding<Item?>, onDismiss: (() -> Void)? = nil, @ViewBuilder content: (Item) -> S) -> some View {
        _SheetView(content: self, isPresented: _itemBinding(item), sheet: item.wrappedValue.map(content), onDismiss: onDismiss, cover: false)
    }
    public func fullScreenCover<S: View>(isPresented: Binding<Bool>, onDismiss: (() -> Void)? = nil, @ViewBuilder content: () -> S) -> some View {
        _SheetView(content: self, isPresented: isPresented, sheet: content(), onDismiss: onDismiss, cover: true)
    }
    public func fullScreenCover<Item, S: View>(item: Binding<Item?>, onDismiss: (() -> Void)? = nil, @ViewBuilder content: (Item) -> S) -> some View {
        _SheetView(content: self, isPresented: _itemBinding(item), sheet: item.wrappedValue.map(content), onDismiss: onDismiss, cover: true)
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
}
