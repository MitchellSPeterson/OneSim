#if os(WASI)
import WASILibc
#else
import Foundation
#endif

// Forms and containers (docs/protocol.md section T): List and Form styles, sections, edit mode, swipe actions,
// LabeledContent, DisclosureGroup, OutlineGroup, GroupBox, ControlGroup, ContentUnavailableView and Table.
// Lists are a host primitive (`list`); everything else is built here from existing nodes.

// MARK: - List styles

public protocol ListStyle {
    var _listStyleName: String { get }
}
/// `.automatic` on iPhone: inset grouped.
public struct DefaultListStyle: ListStyle {
    public init() {}
    public var _listStyleName: String { "insetGrouped" }
}
public struct InsetGroupedListStyle: ListStyle {
    public init() {}
    public var _listStyleName: String { "insetGrouped" }
}
public struct GroupedListStyle: ListStyle {
    public init() {}
    public var _listStyleName: String { "grouped" }
}
public struct PlainListStyle: ListStyle {
    public init() {}
    public var _listStyleName: String { "plain" }
}
public struct InsetListStyle: ListStyle {
    public init() {}
    public var _listStyleName: String { "inset" }
}
public struct SidebarListStyle: ListStyle {
    public init() {}
    public var _listStyleName: String { "sidebar" }
}
extension ListStyle where Self == DefaultListStyle { public static var automatic: DefaultListStyle { .init() } }
extension ListStyle where Self == InsetGroupedListStyle { public static var insetGrouped: InsetGroupedListStyle { .init() } }
extension ListStyle where Self == GroupedListStyle { public static var grouped: GroupedListStyle { .init() } }
extension ListStyle where Self == PlainListStyle { public static var plain: PlainListStyle { .init() } }
extension ListStyle where Self == InsetListStyle { public static var inset: InsetListStyle { .init() } }
extension ListStyle where Self == SidebarListStyle { public static var sidebar: SidebarListStyle { .init() } }

extension View {
    public func listStyle<S: ListStyle>(_ style: S) -> some View { _CtxView(content: self) { $0.styles.listStyle = style._listStyleName } }
}

// MARK: - Edit mode

public enum EditMode: Hashable, Sendable {
    case inactive, transient, active
    public var isEditing: Bool { self != .inactive }
}

struct _EditModeKey: EnvironmentKey {
    static var defaultValue: Binding<EditMode>? { nil }
}
extension EnvironmentValues {
    /// The edit mode of the enclosing screen (each NavigationStack screen, and the root, has its own).
    public var editMode: Binding<EditMode>? {
        get { self[_EditModeKey.self] }
        set { self[_EditModeKey.self] = newValue }
    }
}

final class _EditModeBox { var mode = EditMode.inactive }

/// Give the views evaluated in `f` a screen's own edit mode (kept in the state registry under `key`), unless an
/// ancestor already set `\.editMode`.
func _withEditMode<T>(_ key: String, _ f: () -> T) -> T {
    let rt = _Runtime.shared, k = ObjectIdentifier(_EditModeKey.self)
    let slot = key + "#EditMode"
    rt.touched.insert(slot)
    let box = (rt.slots[slot] as? _EditModeBox) ?? _EditModeBox()
    rt.slots[slot] = box
    if rt.envObjects[k] != nil { return f() }
    let saved = rt.envObjects
    rt.envObjects[k] = Binding<EditMode>?.some(Binding(get: { box.mode }, set: { box.mode = $0; _Runtime.shared.markDirty() }))
    defer { rt.envObjects = saved }
    return f()
}

func _currentEditMode() -> Binding<EditMode>? { EnvironmentValues().editMode }

/// Toggles the screen's edit mode: "Edit", then "Done" while editing.
public struct EditButton: View {
    @Environment(\.editMode) private var editMode
    public init() {}
    public var body: some View {
        let editing = editMode?.wrappedValue.isEditing == true
        Button(editing ? "Done" : "Edit") {
            guard let m = editMode else { return }
            withAnimation { m.wrappedValue = m.wrappedValue.isEditing ? .inactive : .active }
        }
    }
}

// MARK: - Row actions (onDelete, onMove, swipeActions)

/// What a list row offers, keyed by the row node's id: its ForEach's delete and move handlers and its swipe actions.
struct _RowMeta {
    var delete: (() -> Void)?
    /// Position in its ForEach, element count, and `onMove`.
    var move: (index: Int, count: Int, perform: (IndexSet, Int) -> Void)?
    var leading: [Node] = [], trailing: [Node] = []
    var fullLeading = true, fullTrailing = true
    var hasLeading = false, hasTrailing = false
    /// Registered under `.disabled(true)`: no delete control, Delete action, reorder or swipe.
    var disabled = false
}

/// Views whose elements a List can delete and move (`ForEach`).
public protocol DynamicViewContent: View {
    associatedtype Data: Collection
    var data: Data { get }
}

/// `onDelete` / `onMove` handlers for the ForEach inside `content`.
struct _EditActions<Content: DynamicViewContent>: DynamicViewContent, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content
    var delete: ((IndexSet) -> Void)?, move: ((IndexSet, Int) -> Void)?
    var data: Content.Data { content.data }
    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared, saved = rt.rowActions
        var actions = rt.rowActions
        if let delete { actions.delete = delete }
        if let move { actions.move = move }
        rt.rowActions = actions
        defer { rt.rowActions = saved }
        return SwiftUI._children(content, path)
    }
}

extension DynamicViewContent {
    /// Swipe-to-delete, and the delete control in edit mode, for each element's rows in a List.
    public func onDelete(perform action: ((IndexSet) -> Void)?) -> some DynamicViewContent {
        _EditActions(content: self, delete: action, move: nil)
    }
    /// The reorder handle in edit mode: dragging it moves the element (`onMove(fromOffsets, toOffset)`).
    public func onMove(perform action: ((IndexSet, Int) -> Void)?) -> some DynamicViewContent {
        _EditActions(content: self, delete: nil, move: action)
    }
}

/// Register the rows a ForEach element produced: `index` of `count` elements.
func _registerElementRows(_ nodes: [Node], index: Int, count: Int) {
    let rt = _Runtime.shared
    let actions = rt.rowActions
    guard actions.delete != nil || actions.move != nil else { return }
    for n in nodes {
        guard let id = n["id"] as? String else { continue }
        var meta = rt.rowMeta[id] ?? _RowMeta()
        if let d = actions.delete { meta.delete = { d(IndexSet(integer: index)) } }
        if let m = actions.move { meta.move = (index, count, m) }
        if rt.disabled { meta.disabled = true }
        rt.rowMeta[id] = meta
    }
}

public enum HorizontalEdge: Int8, CaseIterable, Hashable, Sendable {
    case leading, trailing
}

struct _SwipeActionsView<Content: View, Actions: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, edge: HorizontalEdge, allowsFullSwipe: Bool, actions: Actions
    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared
        let nodes = _modTargets(content, path)
        let side = edge == .leading ? "leading" : "trailing"
        // Each `.swipeActions` on a row adds to that edge's buttons, outermost first.
        let buttons = _without({ $0.button = nil; $0.inList = false }) { SwiftUI._children(actions, path + "#swipe-" + side) }
        for n in nodes {
            guard let id = n["id"] as? String else { continue }
            var meta = rt.rowMeta[id] ?? _RowMeta()
            if edge == .leading { meta.leading += buttons; meta.fullLeading = allowsFullSwipe; meta.hasLeading = true }
            else { meta.trailing += buttons; meta.fullTrailing = allowsFullSwipe; meta.hasTrailing = true }
            if rt.disabled { meta.disabled = true }
            rt.rowMeta[id] = meta
        }
        return nodes
    }
}

extension View {
    /// Buttons revealed by swiping the list row (trailing by default). The first button is the outermost one, which a
    /// full swipe runs when `allowsFullSwipe`.
    public func swipeActions<T: View>(edge: HorizontalEdge = .trailing, allowsFullSwipe: Bool = true,
                                      @ViewBuilder content: () -> T) -> some View {
        _SwipeActionsView(content: self, edge: edge, allowsFullSwipe: allowsFullSwipe, actions: content())
    }
}

/// Live swipe state of one row (kept across evaluations).
final class _SwipeState {
    var open: HorizontalEdge?
    var offset = 0.0
    var full = false
    var editing = false
}
final class _ReorderState { var lift = 0.0 }

/// The open row in this evaluation's lists: a swipe starting on another row closes it first.
nonisolated(unsafe) var _openSwipe: _SwipeState?

/// A finger moved this far (pt) across the row opens or closes its actions on release.
let _swipeOpenDistance = 30.0
/// Past this fraction of the row's width a full swipe runs the outermost action.
let _fullSwipeFraction = 0.55

/// Horizontal pan on a row: moves it, then opens one edge's actions, closes them, or runs a full swipe.
final class _SwipeR: _GestureRecognizer<Void> {
    let st: _SwipeState, leading: Bool, trailing: Bool, fullLeading: (() -> Void)?, fullTrailing: (() -> Void)?
    var start = CGPoint(), width: CGFloat = 0
    override var pans: Bool { true }
    init(_ st: _SwipeState, leading: Bool, trailing: Bool, fullLeading: (() -> Void)?, fullTrailing: (() -> Void)?) {
        self.st = st; self.leading = leading; self.trailing = trailing; self.fullLeading = fullLeading; self.fullTrailing = fullTrailing
    }
    override func _feed(_ t: _Touch, _ q: _Queue) {
        guard let p = t.points.first else { return fail() }
        let st = self.st
        switch t.phase {
        case .start:
            start = p; width = t.size.width
            status = .tentative
        case .move:
            let dx = Double(p.x - start.x)
            if status != .active {
                guard abs(dx) >= Double(_tapSlop) else { return }
                status = .active
                if let other = _openSwipe, other !== st { q.actions.append { other.open = nil; other.offset = 0; other.full = false } }
            }
            // An edge without actions does not move (beyond closing the other edge).
            var off = dx
            if st.open == nil { off = min(trailing ? off : 0, 0) + max(leading ? dx : 0, 0) }
            let shown = off + (st.open == .trailing ? -Double(width) * 0.4 : st.open == .leading ? Double(width) * 0.4 : 0)
            let full = (shown < -Double(width) * _fullSwipeFraction && fullTrailing != nil) || (shown > Double(width) * _fullSwipeFraction && fullLeading != nil)
            q.actions.append { st.offset = off; st.full = full; _Runtime.shared.markDirty() }
        case .end:
            let dx = Double(p.x - start.x)
            if status != .active {
                // A tap on an open row closes it.
                if st.open != nil { q.actions.append { st.open = nil; st.offset = 0; _Runtime.shared.markDirty() } }
                status = .ended
                return
            }
            let full = st.full
            let run = full ? (dx < 0 ? fullTrailing : fullLeading) : nil
            q.actions.append {
                st.offset = 0; st.full = false
                if let run { st.open = nil; run() }
                else if dx <= -_swipeOpenDistance { st.open = st.open == .leading ? nil : (self.trailing ? .trailing : nil) }
                else if dx >= _swipeOpenDistance { st.open = st.open == .trailing ? nil : (self.leading ? .leading : nil) }
                _openSwipe = st.open == nil ? nil : st
                _Runtime.shared.markDirty()
            }
            status = .ended
        case .cancel:
            q.resets.append { st.offset = 0; st.full = false }
            fail()
        }
    }
}

/// Vertical pan on the reorder handle strip: the row follows the finger, then moves by whole rows of its own height.
final class _ReorderR: _GestureRecognizer<Void> {
    let st: _ReorderState, index: Int, count: Int, perform: (IndexSet, Int) -> Void
    var start = CGPoint(), height: CGFloat = 1
    override var pans: Bool { true }
    init(_ st: _ReorderState, _ index: Int, _ count: Int, _ perform: @escaping (IndexSet, Int) -> Void) {
        self.st = st; self.index = index; self.count = count; self.perform = perform
    }
    override func _feed(_ t: _Touch, _ q: _Queue) {
        guard let p = t.points.first else { return fail() }
        let st = self.st
        switch t.phase {
        case .start:
            start = p; height = max(1, t.size.height); status = .active
        case .move:
            let dy = Double(p.y - start.y)
            q.actions.append { st.lift = dy; _Runtime.shared.markDirty() }
        case .end:
            let steps = Int((Double(p.y - start.y) / Double(height)).rounded())
            let to = min(max(index + steps, 0), count - 1)
            let (i, perform) = (index, perform)
            q.actions.append {
                st.lift = 0
                // `toOffset` counts positions before the move: moving down lands after the destination element.
                if to != i { withAnimation { perform(IndexSet(integer: i), to > i ? to + 1 : to) } }
                _Runtime.shared.markDirty()
            }
            status = .ended
        case .cancel:
            q.resets.append { st.lift = 0 }
            fail()
        }
    }
}

/// A swipe action button node (`SwipeAction` on the wire): its label, action, role and `.tint` color.
func _swipeAction(_ b: Node, close: @escaping () -> Void) -> Node? {
    guard b["kind"] as? String == "button", let label = b["label"] as? Node else { return nil }
    let rt = _Runtime.shared
    let inner = rt.actions[(b["action"] as? UInt64) ?? 0]
    var a: Node = ["label": label, "action": rt.registerAction { close(); inner?() }]
    if let role = b["role"] as? String, role == "destructive" { a["role"] = role }
    for m in (b["modifiers"] as? [Node]) ?? [] {
        if let c = m["tint"] as? Node, c["r"] != nil || c["semantic"] != nil { a["tint"] = c }
    }
    return a
}

/// A list row with its ForEach and swipe actions applied: edit-mode controls, swipe state and gestures.
func _decorateRow(_ row: Node, editing: Bool) -> Node {
    let rt = _Runtime.shared
    guard let id = row["id"] as? String, let meta = rt.rowMeta[id] else { return row }
    var n = row
    var lr: Node = [:]
    let swipeKey = id + "#Swipe"
    rt.touched.insert(swipeKey)
    let st = (rt.slots[swipeKey] as? _SwipeState) ?? _SwipeState()
    rt.slots[swipeKey] = st
    // Entering or leaving edit mode closes the row, and so does disabling it.
    let off = meta.disabled || rt.disabled
    if st.editing != editing || off { st.open = nil; st.offset = 0; st.full = false; st.editing = editing }
    let close = { st.open = nil; st.offset = 0; st.full = false; if _openSwipe === st { _openSwipe = nil }; rt.markDirty() }
    let deleteButton: Node? = off ? nil : meta.delete.map { del in
        ["kind": "button", "id": id + "#delete", "action": rt.registerAction { withAnimation { del() } }, "role": "destructive",
         "label": _eval(Label("Delete", systemImage: "trash"), id + "#delete/l")]
    }
    var leading: [Node] = [], trailing: [Node] = []
    if editing {
        // Edit mode: a delete control opens the row's Delete action; the handle reorders. No swipe actions.
        if meta.delete == nil || st.open == .leading { st.open = nil }
        if let deleteButton { trailing = [deleteButton] }
        var kids: [Node] = []
        if meta.delete != nil {
            // A red disc with a white bar (drawn, so it looks the same with any symbol set).
            let minus = _eval(ZStack {
                Circle().fill(Color.red).frame(width: _editControl.disc, height: _editControl.disc)
                Capsule().fill(Color.white).frame(width: _editControl.bar.0, height: _editControl.bar.1)
            }.frame(width: _editControl.minus, height: _editControl.minus), id + "#minus/i")
            kids.append(["kind": "button", "id": id + "#minus", "style": "plain", "disabled": off, "label": minus,
                         "action": off ? 0 : rt.registerAction { st.open = st.open == nil ? .trailing : nil; _openSwipe = st.open == nil ? nil : st; rt.markDirty() }])
        }
        kids.append(row)
        if let move = meta.move {
            kids.append(["kind": "spacer", "id": id + "#edit/s", "minLength": 8.0])
            kids.append(_eval(Image(systemName: "line.3.horizontal").font(.system(size: _editControl.handle)).foregroundStyle(Color(json: ["semantic": "tertiary"])),
                              id + "#handle"))
            let rk = id + "#Reorder"
            rt.touched.insert(rk)
            let rs = (rt.slots[rk] as? _ReorderState) ?? _ReorderState()
            rt.slots[rk] = rs
            if off { rs.lift = 0 } else {
                let key = id + "#reorder"
                rt.gestures[key] = _GestureEntry(kind: .normal) { _ in _ReorderR(rs, move.index, move.count, move.perform) }
                lr["reorder"] = ["key": key, "pans": true, "axis": "vertical"] as Node
                if rs.lift != 0 { lr["lift"] = rs.lift }
            }
        }
        n = ["kind": "hstack", "id": id + "#edit", "spacing": _editControl.gap, "alignment": "center", "children": kids]
    } else if !off {
        leading = meta.hasLeading ? meta.leading : []
        trailing = meta.hasTrailing ? meta.trailing : (deleteButton.map { [$0] } ?? [])
        if leading.isEmpty && st.open == .leading { st.open = nil }
        if trailing.isEmpty && st.open == .trailing { st.open = nil }
        if !(leading.isEmpty && trailing.isEmpty) {
            let full = { (a: [Node], allowed: Bool) -> (() -> Void)? in
                guard allowed, let first = a.first, let act = rt.actions[(first["action"] as? UInt64) ?? 0] else { return nil }
                return act
            }
            let fl = full(leading, meta.fullLeading), ft = full(trailing, meta.hasTrailing ? meta.fullTrailing : true)
            let (hasL, hasT) = (!leading.isEmpty, !trailing.isEmpty)
            let key = id + "#swipe"
            rt.gestures[key] = _GestureEntry(kind: .normal) { _ in _SwipeR(st, leading: hasL, trailing: hasT, fullLeading: fl, fullTrailing: ft) }
            lr["gesture"] = ["key": key, "pans": true, "axis": "horizontal"] as Node
        }
    }
    let l = leading.compactMap { _swipeAction($0, close: close) }, t = trailing.compactMap { _swipeAction($0, close: close) }
    if !(l.isEmpty && t.isEmpty) {
        var s: Node = [:]
        if !l.isEmpty { s["leading"] = l }
        if !t.isEmpty { s["trailing"] = t }
        if let open = st.open { s["open"] = open == .leading ? "leading" : "trailing" }
        if st.offset != 0 { s["offset"] = st.offset }
        if st.full { s["full"] = true }
        lr["swipe"] = s
    }
    if !lr.isEmpty { n["modifiers"] = ((n["modifiers"] as? [Node]) ?? []) + [["listRow": lr] as Node] }
    return n
}

// MARK: - List

/// Group evaluated list children into sections: `_section` nodes as they are, loose rows into header-less sections.
func _listSections(_ nodes: [Node]) -> [Node] {
    var sections: [Node] = [], loose: [Node] = []
    for n in nodes {
        if n["kind"] as? String == "_section" {
            if !loose.isEmpty { sections.append(["rows": loose]); loose = [] }
            var s = n; s["kind"] = nil; s["id"] = nil; sections.append(s)
        } else { loose.append(n) }
    }
    if !loose.isEmpty { sections.append(["rows": loose]) }
    return sections
}

public struct List<Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let content: Content
    public init(@ViewBuilder content: () -> Content) { self.content = content() }
    func _emit(_ path: String) -> Node { _list(path, style: _Runtime.shared.styles.listStyle ?? "insetGrouped") { _children(content, path) } }
}

/// A `list` node of `rows()` (evaluated as list rows) in `style`.
func _list(_ path: String, style: String, _ rows: () -> [Node]) -> Node {
    let editing = _currentEditMode()?.wrappedValue.isEditing ?? false
    let nodes = _without({ $0.inList = true; $0.listStyle = nil }) { rows() }
    var sections = _listSections(nodes)
    for i in sections.indices {
        sections[i]["rows"] = ((sections[i]["rows"] as? [Node]) ?? []).map { _decorateRow($0, editing: editing) }
    }
    return ["kind": "list", "id": path, "style": style, "sections": sections]
}

extension List {
    public init<Data: RandomAccessCollection, ID: Hashable, RowContent: View>(
        _ data: Data, id: KeyPath<Data.Element, ID>, @ViewBuilder rowContent: @escaping (Data.Element) -> RowContent
    ) where Content == ForEach<Data, ID, RowContent> {
        self.init { ForEach(data, id: id, content: rowContent) }
    }
    public init<Data: RandomAccessCollection, RowContent: View>(
        _ data: Data, @ViewBuilder rowContent: @escaping (Data.Element) -> RowContent
    ) where Content == ForEach<Data, Data.Element.ID, RowContent>, Data.Element: Identifiable {
        self.init { ForEach(data, content: rowContent) }
    }
    public init<RowContent: View>(_ data: Range<Int>, @ViewBuilder rowContent: @escaping (Int) -> RowContent)
    where Content == ForEach<Range<Int>, Int, RowContent> {
        self.init { ForEach(data, content: rowContent) }
    }
    /// A hierarchical list: elements with `children` are disclosure rows (`OutlineGroup`).
    public init<Data: RandomAccessCollection, RowContent: View>(
        _ data: Data, children: KeyPath<Data.Element, Data?>, @ViewBuilder rowContent: @escaping (Data.Element) -> RowContent
    ) where Content == OutlineGroup<Data, Data.Element.ID, RowContent, RowContent, DisclosureGroup<RowContent, OutlineSubgroupChildren>>,
            Data.Element: Identifiable {
        self.init { OutlineGroup(data, children: children, content: rowContent) }
    }
    public init<Data: RandomAccessCollection, ID: Hashable, RowContent: View>(
        _ data: Data, id: KeyPath<Data.Element, ID>, children: KeyPath<Data.Element, Data?>,
        @ViewBuilder rowContent: @escaping (Data.Element) -> RowContent
    ) where Content == OutlineGroup<Data, ID, RowContent, RowContent, DisclosureGroup<RowContent, OutlineSubgroupChildren>> {
        self.init { OutlineGroup(data, id: id, children: children, content: rowContent) }
    }
}

// MARK: - Section

public struct Section<Parent: View, Content: View, Footer: View>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let header: Parent?, footer: Footer?, content: Content
    var isExpanded: Binding<Bool>?
    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared
        var n: Node = ["kind": "_section", "id": path, "rows": SwiftUI._children(content, path)]
        let outside = { (f: () -> [Node]) in _without({ $0.inList = false }, f) }
        func put(_ v: (some View)?, _ text: String, _ view: String, _ p: String) {
            guard let v else { return }
            let c = outside { SwiftUI._children(v, p) }
            if c.isEmpty { return }
            if c.count == 1, c[0]["kind"] as? String == "text", c[0]["modifiers"] == nil, (c[0]["runs"] as? [Any])?.isEmpty ?? true,
               let t = c[0]["text"] as? String {
                n[text] = t
            } else {
                n[view] = c.count == 1 ? c[0] : ["kind": "vstack", "id": p, "spacing": 0.0, "alignment": "leading", "children": c]
            }
        }
        put(header, "header", "headerView", path + "#header")
        put(footer, "footer", "footerView", path + "#footer")
        if let isExpanded {
            n["expanded"] = isExpanded.wrappedValue
            n["toggle"] = rt.registerAction { withAnimation { isExpanded.wrappedValue.toggle() } }
            if !isExpanded.wrappedValue { n["rows"] = [Node]() }
        }
        return [n]
    }
}

extension Section where Parent == EmptyView, Footer == EmptyView {
    public init(@ViewBuilder content: () -> Content) { header = nil; footer = nil; self.content = content() }
}
extension Section where Parent == Text, Footer == EmptyView {
    public init(_ title: String, @ViewBuilder content: () -> Content) { header = Text(title); footer = nil; self.content = content() }
    /// A collapsible section (a chevron in the header in sidebar lists).
    public init(_ title: String, isExpanded: Binding<Bool>, @ViewBuilder content: () -> Content) {
        header = Text(title); footer = nil; self.content = content(); self.isExpanded = isExpanded
    }
}
extension Section where Footer == EmptyView {
    public init(@ViewBuilder content: () -> Content, @ViewBuilder header: () -> Parent) {
        self.header = header(); footer = nil; self.content = content()
    }
    public init(header: Parent, @ViewBuilder content: () -> Content) { self.header = header; footer = nil; self.content = content() }
    public init(isExpanded: Binding<Bool>, @ViewBuilder content: () -> Content, @ViewBuilder header: () -> Parent) {
        self.header = header(); footer = nil; self.content = content(); self.isExpanded = isExpanded
    }
}
extension Section where Parent == EmptyView {
    public init(@ViewBuilder content: () -> Content, @ViewBuilder footer: () -> Footer) {
        header = nil; self.footer = footer(); self.content = content()
    }
    public init(footer: Footer, @ViewBuilder content: () -> Content) { header = nil; self.footer = footer; self.content = content() }
}
extension Section {
    public init(@ViewBuilder content: () -> Content, @ViewBuilder header: () -> Parent, @ViewBuilder footer: () -> Footer) {
        self.header = header(); self.footer = footer(); self.content = content()
    }
    public init(header: Parent, footer: Footer, @ViewBuilder content: () -> Content) {
        self.header = header; self.footer = footer; self.content = content()
    }
}

// MARK: - Form

public struct FormStyleConfiguration {
    public typealias Content = _StyleLabel
    public let content: Content
}

public protocol FormStyle {
    associatedtype Body: View
    typealias Configuration = FormStyleConfiguration
    @ViewBuilder func makeBody(configuration: Configuration) -> Body
    var _formStyleName: String? { get }
}
extension FormStyle { public var _formStyleName: String? { nil } }

/// `.automatic` on iPhone: grouped.
public struct AutomaticFormStyle: FormStyle {
    public init() {}
    public var _formStyleName: String? { "grouped" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
public struct GroupedFormStyle: FormStyle {
    public init() {}
    public var _formStyleName: String? { "grouped" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
/// Labels trailing-aligned in a leading column, controls in a trailing one, centred on the screen.
public struct ColumnsFormStyle: FormStyle {
    public init() {}
    public var _formStyleName: String? { "columns" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
extension FormStyle where Self == AutomaticFormStyle { public static var automatic: AutomaticFormStyle { .init() } }
extension FormStyle where Self == GroupedFormStyle { public static var grouped: GroupedFormStyle { .init() } }
extension FormStyle where Self == ColumnsFormStyle { public static var columns: ColumnsFormStyle { .init() } }

extension View {
    public func formStyle<S: FormStyle>(_ style: S) -> some View {
        _CtxView(content: self) { $0.styles.form = _named(style._formStyleName) { cfg, p in _eval(style.makeBody(configuration: cfg), p) } }
    }
}

public struct Form<Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let content: Content?
    let configuration: FormStyleConfiguration?
    public init(@ViewBuilder content: () -> Content) { self.content = content(); configuration = nil }
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared
        let rows: () -> [Node] = { configuration.map { $0.content.make(path) } ?? SwiftUI._children(content!, path) }
        switch rt.styles.form {
        case .custom(let f)? where configuration == nil:
            let cfg = FormStyleConfiguration(content: _StyleLabel { p in _without({ $0.form = nil }) { SwiftUI._children(content!, p) } })
            return _without({ $0.form = nil }) { f(cfg, path) }
        case .name("columns")?:
            return _columnsForm(path, rows)
        default: break
        }
        // A grouped form is an inset-grouped list.
        return _list(path, style: "insetGrouped", rows)
    }
}
extension Form where Content == FormStyleConfiguration.Content {
    /// The form a custom `FormStyle` wraps, drawn in the default (grouped) style.
    public init(_ configuration: FormStyleConfiguration) { content = nil; self.configuration = configuration }
}

/// Columns form: each section's header over the trailing column, then label | control rows (labels trailing-aligned),
/// the whole centred in the screen when it fits and scrolling otherwise.
func _columnsForm(_ path: String, _ rows: () -> [Node]) -> Node {
    let nodes = _without({ $0.columnsForm = true }) { rows() }
    var cells: [(left: Node?, right: Node, control: Bool)] = []
    for (si, s) in _listSections(nodes).enumerated() {
        if let h = (s["header"] as? String).map({ ["kind": "text", "id": "\(path)#s\(si)/h", "text": $0] as Node }) ?? (s["headerView"] as? Node) {
            cells.append((nil, h, false))
        }
        for r in (s["rows"] as? [Node]) ?? [] {
            var r = r
            if let pair = r["_columns"] as? [Node], pair.count == 2 {
                cells.append((pair[0], pair[1], false))
            } else if r["kind"] as? String == "toggle", let label = r["label"] as? Node {
                // The label on the left; the switch's top on the label's baseline on the right.
                r["label"] = ["kind": "empty", "id": (r["id"] as? String ?? path) + "/hidden"] as Node
                r["modifiers"] = ((r["modifiers"] as? [Node]) ?? []) + [
                    ["frame": ["width": _columnsSwitch.width, "height": _columnsSwitch.height, "alignment": "leading"] as Node] as Node,
                    ["padding": ["top": _columnsSwitch.top, "leading": 0.0, "bottom": 0.0, "trailing": 0.0] as Node] as Node]
                cells.append((label, r, true))
            } else {
                r["_columns"] = nil
                cells.append((nil, r, r["kind"] as? String != "text"))
            }
        }
    }
    var gridRows: [Node] = []
    for (i, c) in cells.enumerated() {
        // Text rows sit 2pt apart; a control row is 10.67pt below the row above it and 6.33pt above the next.
        let gap = i == 0 ? 0 : cells[i - 1].control ? _columnsGap.afterControl : c.control ? _columnsGap.beforeControl : _columnsGap.text
        let pad: Node = ["padding": ["top": gap, "leading": 0.0, "bottom": 0.0, "trailing": 0.0] as Node]
        var left: Node = c.left ?? ["kind": "empty", "id": "\(path)#c\(i)/e"]
        left["_columns"] = nil
        left["modifiers"] = ((left["modifiers"] as? [Node]) ?? []) + [["frame": ["maxWidth": "inf", "alignment": "trailing"] as Node] as Node, pad]
        var right = c.right
        right["_columns"] = nil
        right["modifiers"] = ((right["modifiers"] as? [Node]) ?? []) + [pad]
        gridRows.append(["alignment": "topLeading", "cells": [left, right]])
    }
    let grid: Node = ["kind": "grid", "id": path + "/grid", "alignment": "topLeading", "horizontalSpacing": _columnsGap.columns,
                      "verticalSpacing": 0.0, "rows": gridRows]
    var centred = grid
    centred["modifiers"] = [["frame": ["maxWidth": "inf", "maxHeight": "inf", "alignment": "center"] as Node] as Node]
    var scrolled = grid
    scrolled["id"] = path + "/sgrid"
    scrolled["modifiers"] = [["frame": ["maxWidth": "inf", "alignment": "top"] as Node] as Node,
                             ["padding": ["top": 20.0, "leading": 0.0, "bottom": 20.0, "trailing": 0.0] as Node] as Node]
    let scroll: Node = ["kind": "scroll", "id": path + "/scroll", "axis": "vertical", "indicators": "automatic", "child": scrolled]
    return ["kind": "viewThatFits", "id": path, "axes": "vertical", "children": [centred, scroll]]
}
/// Columns form spacing (iOS 27.2, swiftui-forms columns).
let _columnsGap = (text: 2.0, beforeControl: 10.67, afterControl: 6.33, columns: 8.67)
let _columnsSwitch = (width: 61.0, height: 28.0, top: 17.0)

// MARK: - LabeledContent

public struct LabeledContentStyleConfiguration {
    public typealias Label = _StyleLabel
    public typealias Content = _StyleLabel
    public let label: Label
    public let content: Content
}

public protocol LabeledContentStyle {
    associatedtype Body: View
    typealias Configuration = LabeledContentStyleConfiguration
    @ViewBuilder func makeBody(configuration: Configuration) -> Body
    var _isAutomatic: Bool { get }
}
extension LabeledContentStyle { public var _isAutomatic: Bool { false } }
public struct AutomaticLabeledContentStyle: LabeledContentStyle {
    public init() {}
    public var _isAutomatic: Bool { true }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
extension LabeledContentStyle where Self == AutomaticLabeledContentStyle { public static var automatic: AutomaticLabeledContentStyle { .init() } }

extension View {
    public func labeledContentStyle<S: LabeledContentStyle>(_ style: S) -> some View {
        _CtxView(content: self) { $0.styles.labeledContent = style._isAutomatic ? nil : { cfg, p in _eval(style.makeBody(configuration: cfg), p) } }
    }
}

/// Label leading, content trailing in the secondary style (a list row's trailing value). A columns form splits them.
public struct LabeledContent<Label: View, Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let label: Label?, content: Content?
    let configuration: LabeledContentStyleConfiguration?
    public init(@ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label) {
        self.content = content(); self.label = label(); configuration = nil
    }
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared
        let makeLabel: (String) -> [Node] = configuration.map { c in { c.label.make($0) } } ?? { SwiftUI._children(label!, $0) }
        let makeContent: (String) -> [Node] = configuration.map { c in { c.content.make($0) } } ?? { SwiftUI._children(content!, $0) }
        if configuration == nil, let f = rt.styles.labeledContent {
            let cfg = LabeledContentStyleConfiguration(label: _StyleLabel(make: makeLabel), content: _StyleLabel(make: makeContent))
            return _without({ $0.labeledContent = nil }) { f(cfg, path) }
        }
        let l = _group(makeLabel(path + "/l"), path + "/l")
        var c = _group(_without({ $0.columnsForm = false }) { makeContent(path + "/c") }, path + "/c")
        // `.foregroundStyle(.secondary)` around the content: its own styles still win.
        c["modifiers"] = ((c["modifiers"] as? [Node]) ?? []) + [["foregroundColor": ["semantic": "secondary", "hierarchical": true]] as Node]
        if rt.styles.columnsForm {
            // Hand the columns form its two cells.
            return ["kind": "hstack", "id": path, "spacing": 8.0, "alignment": "center", "children": [l, c], "_columns": [l, c]]
        }
        return ["kind": "hstack", "id": path, "spacing": NSNull(), "alignment": "center",
                "children": [l, ["kind": "spacer", "id": path + "/s", "minLength": NSNull()] as Node, c]]
    }
}

/// One node for a view's children (a leading vstack when there are several).
func _group(_ c: [Node], _ path: String) -> Node {
    if c.count == 1 { return c[0] }
    if c.isEmpty { return ["kind": "empty", "id": path] }
    return ["kind": "vstack", "id": path, "spacing": 0.0, "alignment": "leading", "children": c]
}

extension LabeledContent where Label == Text {
    public init(_ title: String, @ViewBuilder content: () -> Content) { self.init(content: content) { Text(title) } }
}
extension LabeledContent where Label == Text, Content == Text {
    public init<S: StringProtocol>(_ title: String, value: S) { self.init(title) { Text(String(value)) } }
    #if !os(WASI)
    /// The value formatted with `format` (e.g. `.number`, `.currency(code:)`).
    public init<F: FormatStyle>(_ title: String, value: F.FormatInput, format: F) where F.FormatInput: Equatable, F.FormatOutput == String {
        self.init(title) { Text(format.format(value)) }
    }
    #endif
}
extension LabeledContent where Label == LabeledContentStyleConfiguration.Label, Content == LabeledContentStyleConfiguration.Content {
    /// The labeled content a custom style wraps, drawn in the default style.
    public init(_ configuration: LabeledContentStyleConfiguration) { label = nil; content = nil; self.configuration = configuration }
}

// MARK: - DisclosureGroup

public struct DisclosureGroupStyleConfiguration {
    public typealias Label = _StyleLabel
    public typealias Content = _StyleLabel
    public let label: Label
    public let content: Content
    @Binding public var isExpanded: Bool
}

public protocol DisclosureGroupStyle {
    associatedtype Body: View
    typealias Configuration = DisclosureGroupStyleConfiguration
    @ViewBuilder func makeBody(configuration: Configuration) -> Body
    var _isAutomatic: Bool { get }
}
extension DisclosureGroupStyle { public var _isAutomatic: Bool { false } }
public struct AutomaticDisclosureGroupStyle: DisclosureGroupStyle {
    public init() {}
    public var _isAutomatic: Bool { true }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
extension DisclosureGroupStyle where Self == AutomaticDisclosureGroupStyle { public static var automatic: AutomaticDisclosureGroupStyle { .init() } }

extension View {
    public func disclosureGroupStyle<S: DisclosureGroupStyle>(_ style: S) -> some View {
        _CtxView(content: self) { $0.styles.disclosureGroup = style._isAutomatic ? nil : { cfg, p in _eval(style.makeBody(configuration: cfg), p) } }
    }
}

final class _ExpandedBox { var on = false }

/// Leading inset of a disclosure group's (and an outline's) child rows in a list (iOS 27.2, swiftui-forms forms-expanded).
let _disclosureIndent = 20.0

/// A label with a chevron that shows or hides its content. In a list the label is a row and the content rows
/// follow it, indented; elsewhere the label is an accent button above the content.
public struct DisclosureGroup<Label: View, Content: View>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let label: Label, content: () -> Content
    let isExpanded: Binding<Bool>?
    public init(@ViewBuilder content: @escaping () -> Content, @ViewBuilder label: () -> Label) {
        self.content = content; self.label = label(); isExpanded = nil
    }
    public init(isExpanded: Binding<Bool>, @ViewBuilder content: @escaping () -> Content, @ViewBuilder label: () -> Label) {
        self.content = content; self.label = label(); self.isExpanded = isExpanded
    }
    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared
        let expanded: Binding<Bool>
        if let isExpanded { expanded = isExpanded } else {
            let key = path + "#DisclosureGroup"
            rt.touched.insert(key)
            let box = (rt.slots[key] as? _ExpandedBox) ?? _ExpandedBox()
            rt.slots[key] = box
            expanded = Binding(get: { box.on }, set: { box.on = $0; _Runtime.shared.markDirty() })
        }
        let (label, content) = (label, content)
        if let f = rt.styles.disclosureGroup {
            let cfg = DisclosureGroupStyleConfiguration(label: _styleLabel(label), content: _StyleLabel { SwiftUI._children(content(), $0) },
                                                        isExpanded: expanded)
            return [_without({ $0.disclosureGroup = nil }) { f(cfg, path) }]
        }
        return _disclosure(path, expanded, label: { SwiftUI._children(label, $0) }, content: { SwiftUI._children(content(), $0) })
    }
}

/// The automatic disclosure group: header row (or button) plus, when expanded, the content.
func _disclosure(_ path: String, _ expanded: Binding<Bool>, label: (String) -> [Node], content: (String) -> [Node]) -> [Node] {
    let rt = _Runtime.shared
    let on = expanded.wrappedValue
    let toggle = rt.registerAction { withAnimation { expanded.wrappedValue.toggle() } }
    let inList = rt.styles.inList
    let chevron: Node = ["kind": "image", "id": path + "/chev", "systemName": on ? "chevron.down" : "chevron.right", "resizable": false,
                         "modifiers": [["font": ["size": _disclosureChevron, "weight": "semibold"]] as Node,
                                       ["frame": ["width": _disclosureChevronFrame, "alignment": "center"]] as Node] as [Node]]
    let title = _group(_without({ $0.inList = false }) { label(path + "/l") }, path + "/l")
    var header: Node = ["kind": "button", "id": path, "style": inList ? "plain" : "automatic", "disabled": rt.disabled,
                        "action": rt.disabled ? 0 : toggle, "value": on ? "expanded" : "collapsed",
                        "label": ["kind": "hstack", "id": path + "/h", "spacing": NSNull(), "alignment": "center",
                                  "children": [title, ["kind": "spacer", "id": path + "/s", "minLength": NSNull()] as Node, chevron]] as Node]
    if !inList {
        header["modifiers"] = [["padding": ["top": _disclosureHeaderPad, "leading": 0.0, "bottom": _disclosureHeaderPad, "trailing": 0.0] as Node] as Node]
    }
    guard on else { return [header] }
    if inList {
        let rows = content(path + "/c").map { r -> Node in
            var r = r
            r["modifiers"] = ((r["modifiers"] as? [Node]) ?? []) + [["padding": ["top": 0.0, "leading": _disclosureIndent, "bottom": 0.0, "trailing": 0.0]] as Node]
            return r
        }
        return [header] + rows
    }
    let body = content(path + "/c")
    return [["kind": "vstack", "id": path + "/g", "spacing": _disclosureGap, "alignment": "leading",
             "children": [header] + [_group(body, path + "/c")]]]
}
/// Outside a list: the label's vertical padding and its gap to the content (iOS 27.2, swiftui-forms boxes).
let _disclosureHeaderPad = 4.0
let _disclosureGap = 0.0

extension DisclosureGroup where Label == Text {
    public init(_ title: String, @ViewBuilder content: @escaping () -> Content) { self.init(content: content) { Text(title) } }
    public init(_ title: String, isExpanded: Binding<Bool>, @ViewBuilder content: @escaping () -> Content) {
        self.init(isExpanded: isExpanded, content: content) { Text(title) }
    }
}
extension DisclosureGroup where Label == DisclosureGroupStyleConfiguration.Label, Content == DisclosureGroupStyleConfiguration.Content {
    /// The disclosure group a custom style wraps, drawn in the automatic style.
    public init(_ configuration: DisclosureGroupStyleConfiguration) {
        let c = configuration.content
        self.init(isExpanded: configuration.$isExpanded, content: { c }) { configuration.label }
    }
}

// MARK: - OutlineGroup

/// The child rows of one outline element (a `DisclosureGroup`'s content in an `OutlineGroup`).
public struct OutlineSubgroupChildren: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let make: (String) -> [Node]
    func _children(_ path: String) -> [Node] { make(path) }
}

/// A tree: each element with children is a disclosure group of its children's rows, recursively.
public struct OutlineGroup<Data: RandomAccessCollection, ID: Hashable, Parent: View, Leaf: View, Subgroup: View>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let data: [Data.Element], id: KeyPath<Data.Element, ID>, children: KeyPath<Data.Element, Data?>
    let leaf: (Data.Element) -> Leaf
    func _children(_ path: String) -> [Node] {
        data.flatMap { element -> [Node] in
            let p = "\(path)/\(element[keyPath: id])"
            guard let kids = element[keyPath: children] else { return SwiftUI._children(leaf(element), p) }
            let sub = OutlineGroup(data: Array(kids), id: id, children: children, leaf: leaf)
            let row = leaf(element)
            return DisclosureGroup(content: { OutlineSubgroupChildren { sub._children($0) } }) { row }._children(p)
        }
    }
}

extension OutlineGroup where ID == Data.Element.ID, Parent == Leaf, Subgroup == DisclosureGroup<Leaf, OutlineSubgroupChildren>, Data.Element: Identifiable {
    public init(_ data: Data, children: KeyPath<Data.Element, Data?>, @ViewBuilder content: @escaping (Data.Element) -> Leaf) {
        self.init(data: Array(data), id: \.id, children: children, leaf: content)
    }
    /// A tree from one root element.
    public init(_ root: Data.Element, children: KeyPath<Data.Element, Data?>, @ViewBuilder content: @escaping (Data.Element) -> Leaf) {
        self.init(data: [root], id: \.id, children: children, leaf: content)
    }
}
extension OutlineGroup where Parent == Leaf, Subgroup == DisclosureGroup<Leaf, OutlineSubgroupChildren> {
    public init(_ data: Data, id: KeyPath<Data.Element, ID>, children: KeyPath<Data.Element, Data?>,
                @ViewBuilder content: @escaping (Data.Element) -> Leaf) {
        self.init(data: Array(data), id: id, children: children, leaf: content)
    }
    public init(_ root: Data.Element, id: KeyPath<Data.Element, ID>, children: KeyPath<Data.Element, Data?>,
                @ViewBuilder content: @escaping (Data.Element) -> Leaf) {
        self.init(data: [root], id: id, children: children, leaf: content)
    }
}

// MARK: - GroupBox

public struct GroupBoxStyleConfiguration {
    public typealias Label = _StyleLabel
    public typealias Content = _StyleLabel
    public let label: Label
    public let content: Content
}

public protocol GroupBoxStyle {
    associatedtype Body: View
    typealias Configuration = GroupBoxStyleConfiguration
    @ViewBuilder func makeBody(configuration: Configuration) -> Body
    var _isAutomatic: Bool { get }
}
extension GroupBoxStyle { public var _isAutomatic: Bool { false } }
public struct DefaultGroupBoxStyle: GroupBoxStyle {
    public init() {}
    public var _isAutomatic: Bool { true }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
extension GroupBoxStyle where Self == DefaultGroupBoxStyle { public static var automatic: DefaultGroupBoxStyle { .init() } }

extension View {
    public func groupBoxStyle<S: GroupBoxStyle>(_ style: S) -> some View {
        _CtxView(content: self) { $0.styles.groupBox = style._isAutomatic ? nil : { cfg, p in _eval(style.makeBody(configuration: cfg), p) } }
    }
}

/// GroupBox geometry (iOS 27.2, swiftui-forms boxes): 16 pt padding, 8 pt corners, a 10 pt label gap, 8 pt between rows.
let _groupBox = (pad: 16.0, radius: 8.0, labelGap: 10.0, spacing: 8.0)

/// A rounded secondary-background box: a headline label over its content, leading-aligned.
public struct GroupBox<Label: View, Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let label: Label?, content: Content?
    let configuration: GroupBoxStyleConfiguration?
    public init(@ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label) {
        self.content = content(); self.label = label(); configuration = nil
    }
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared
        let makeLabel: (String) -> [Node] = configuration.map { c in { c.label.make($0) } } ?? { SwiftUI._children(label!, $0) }
        let makeContent: (String) -> [Node] = configuration.map { c in { c.content.make($0) } } ?? { SwiftUI._children(content!, $0) }
        if configuration == nil, let f = rt.styles.groupBox {
            let cfg = GroupBoxStyleConfiguration(label: _StyleLabel(make: makeLabel), content: _StyleLabel(make: makeContent))
            return _without({ $0.groupBox = nil }) { f(cfg, path) }
        }
        let depth = rt.styles.groupBoxDepth
        let (l, c) = _without({ $0.inList = false; $0.groupBoxDepth = depth + 1 }) { (makeLabel(path + "/l"), makeContent(path + "/c")) }
        var kids: [Node] = []
        if !l.isEmpty {
            var ln = _group(l, path + "/l")
            ln["modifiers"] = [["font": ["style": "headline"]] as Node] + ((ln["modifiers"] as? [Node]) ?? [])
            kids.append(ln)
        }
        kids.append(["kind": "vstack", "id": path + "/cs", "spacing": _groupBox.spacing, "alignment": "leading", "children": c])
        // Nested boxes alternate between the secondary and tertiary backgrounds.
        let fill = depth % 2 == 0 ? _rgb((242, 242, 247), dark: (28, 28, 30)) : _rgb((255, 255, 255), dark: (44, 44, 46))
        let pad = _groupBox.pad
        let frame: Node = ["frame": ["maxWidth": "inf", "alignment": "leading"] as Node]
        let padding: Node = ["padding": ["top": pad, "leading": pad, "bottom": pad, "trailing": pad] as Node]
        let bg: Node = ["background": ["kind": "color", "id": path + "/bg", "color": fill] as Node]
        let corner: Node = ["cornerRadius": _groupBox.radius]
        return ["kind": "vstack", "id": path, "spacing": _groupBox.labelGap, "alignment": "leading", "children": kids,
                "modifiers": [frame, padding, bg, corner]]
    }
}
extension GroupBox where Label == EmptyView {
    public init(@ViewBuilder content: () -> Content) { self.init(content: content) { EmptyView() } }
}
extension GroupBox where Label == Text {
    public init(_ title: String, @ViewBuilder content: () -> Content) { self.init(content: content) { Text(title) } }
}
extension GroupBox where Label == GroupBoxStyleConfiguration.Label, Content == GroupBoxStyleConfiguration.Content {
    /// The box a custom style wraps, drawn in the default style.
    public init(_ configuration: GroupBoxStyleConfiguration) { label = nil; content = nil; self.configuration = configuration }
}

// MARK: - ControlGroup

public struct ControlGroupStyleConfiguration {
    public typealias Label = _StyleLabel
    public typealias Content = _StyleLabel
    public let label: Label
    public let content: Content
}

public protocol ControlGroupStyle {
    associatedtype Body: View
    typealias Configuration = ControlGroupStyleConfiguration
    @ViewBuilder func makeBody(configuration: Configuration) -> Body
    var _controlGroupName: String? { get }
}
extension ControlGroupStyle { public var _controlGroupName: String? { nil } }
public struct AutomaticControlGroupStyle: ControlGroupStyle {
    public init() {}
    public var _controlGroupName: String? { "automatic" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
/// iOS draws `.navigation` like `.automatic`.
public struct NavigationControlGroupStyle: ControlGroupStyle {
    public init() {}
    public var _controlGroupName: String? { "automatic" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
/// A menu button (the label) listing the controls.
public struct MenuControlGroupStyle: ControlGroupStyle {
    public init() {}
    public var _controlGroupName: String? { "menu" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
/// Outside a menu iOS shows the compact menu as a menu button too.
public struct CompactMenuControlGroupStyle: ControlGroupStyle {
    public init() {}
    public var _controlGroupName: String? { "menu" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
/// Outside a menu the palette is the automatic row of controls.
public struct PaletteControlGroupStyle: ControlGroupStyle {
    public init() {}
    public var _controlGroupName: String? { "automatic" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
extension ControlGroupStyle where Self == AutomaticControlGroupStyle { public static var automatic: AutomaticControlGroupStyle { .init() } }
extension ControlGroupStyle where Self == NavigationControlGroupStyle { public static var navigation: NavigationControlGroupStyle { .init() } }
extension ControlGroupStyle where Self == MenuControlGroupStyle { public static var menu: MenuControlGroupStyle { .init() } }
extension ControlGroupStyle where Self == CompactMenuControlGroupStyle { public static var compactMenu: CompactMenuControlGroupStyle { .init() } }
extension ControlGroupStyle where Self == PaletteControlGroupStyle { public static var palette: PaletteControlGroupStyle { .init() } }

extension View {
    public func controlGroupStyle<S: ControlGroupStyle>(_ style: S) -> some View {
        _CtxView(content: self) { $0.styles.controlGroup = _named(style._controlGroupName) { cfg, p in _eval(style.makeBody(configuration: cfg), p) } }
    }
}

/// ControlGroup geometry (iOS 27.2, swiftui-forms boxes): a 32 pt capsule, equal-width 13 pt medium labels.
let _controlGroup = (height: 32.0, font: 13.0)

/// Related controls side by side in one capsule, each taking an equal share of the width.
public struct ControlGroup<Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let content: Content?, label: (String) -> [Node]
    let configuration: ControlGroupStyleConfiguration?
    public init(@ViewBuilder content: () -> Content) { self.content = content(); label = { _ in [] }; configuration = nil }
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared
        let makeContent: (String) -> [Node] = configuration.map { c in { c.content.make($0) } } ?? { SwiftUI._children(content!, $0) }
        let makeLabel: (String) -> [Node] = configuration.map { c in { c.label.make($0) } } ?? label
        switch rt.styles.controlGroup {
        case .custom(let f)? where configuration == nil:
            let cfg = ControlGroupStyleConfiguration(label: _StyleLabel(make: makeLabel), content: _StyleLabel(make: makeContent))
            return _without({ $0.controlGroup = nil }) { f(cfg, path) }
        case .name("menu")?:
            // A menu whose button is the label (or "…" with no label).
            let l = makeLabel(path + "/ml")
            let labelView: _StyleLabel = l.isEmpty ? _StyleLabel { [["kind": "image", "id": $0, "systemName": "ellipsis", "resizable": false]] } : _StyleLabel { _ in l }
            return Menu(content: { _StyleLabel(make: makeContent) }, label: { labelView })._menu(path)
        default: break
        }
        let controls = _without({ $0.button = nil; $0.inList = false }) { makeContent(path + "/c") }
        let kids = controls.enumerated().map { i, n -> Node in
            var c = n
            // Buttons take the primary color and the group's font; each fills its share.
            if c["kind"] as? String == "button" { c["style"] = "plain" }
            c["modifiers"] = ((c["modifiers"] as? [Node]) ?? []) + [
                ["font": ["size": _controlGroup.font, "weight": "medium"]] as Node,
                ["frame": ["maxWidth": "inf", "minHeight": _controlGroup.height, "alignment": "center"]] as Node]
            return c
        }
        return ["kind": "hstack", "id": path, "spacing": 0.0, "alignment": "center", "children": kids,
                "modifiers": [["background": ["kind": "shape", "id": path + "/bg", "shape": "capsule",
                                              "fill": ["r": 0.0, "g": 0.0, "b": 0.0, "a": 0.07, "dark": [1.0, 1.0, 1.0]]] as Node] as Node] as [Node]]
    }
}
extension ControlGroup {
    public init<C: View, L: View>(@ViewBuilder content: () -> C, @ViewBuilder label: () -> L) where Content == LabeledControlGroupContent<C, L> {
        let l = label()
        self.content = LabeledControlGroupContent(content: content(), label: l); self.label = { SwiftUI._children(l, $0) }; configuration = nil
    }
    public init<C: View>(_ title: String, @ViewBuilder content: () -> C) where Content == LabeledControlGroupContent<C, Text> {
        self.init(content: content) { Text(title) }
    }
    public init<C: View>(_ title: String, systemImage: String, @ViewBuilder content: () -> C) where Content == LabeledControlGroupContent<C, Label<Text, Image>> {
        self.init(content: content) { Label(title, systemImage: systemImage) }
    }
}
extension ControlGroup where Content == ControlGroupStyleConfiguration.Content {
    /// The group a custom style wraps, drawn in the automatic style.
    public init(_ configuration: ControlGroupStyleConfiguration) { content = nil; label = { _ in [] }; self.configuration = configuration }
}

/// A labeled `ControlGroup`'s content: the controls (the label shows only where the group collapses to a menu).
public struct LabeledControlGroupContent<Content: View, Label: View>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let content: Content, label: Label
    func _children(_ path: String) -> [Node] { SwiftUI._children(content, path) }
}

// MARK: - ContentUnavailableView

/// ContentUnavailableView geometry (iOS 27.2, swiftui-forms empty): symbol size, gaps between the parts.
let _unavailable = (symbol: 40.0, titleGap: 18.67, descriptionGap: -2.67, actionsGap: 30.67, noActions: 28.0, pad: 32.0)

/// An empty state: a large secondary symbol, a bold title, a secondary description and actions, centred.
public struct ContentUnavailableView<Label: View, Description: View, Actions: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let label: Label, description: Description, actions: Actions
    public init(@ViewBuilder label: () -> Label, @ViewBuilder description: () -> Description = { EmptyView() },
                @ViewBuilder actions: () -> Actions = { EmptyView() }) {
        self.label = label(); self.description = description(); self.actions = actions()
    }
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared
        var kids: [Node] = []
        // The label's icon above its title (a `Label`), else the label view as the title.
        for n in _without({ $0.inList = false }) { SwiftUI._children(label, path + "/l") } {
            if n["kind"] as? String == "label", let icon = n["icon"] as? Node, let title = n["title"] as? Node {
                var i = icon
                i["modifiers"] = ((i["modifiers"] as? [Node]) ?? []) + [["font": ["size": _unavailable.symbol, "weight": "regular"]] as Node,
                                                                        ["foregroundColor": ["semantic": "secondary", "hierarchical": true]] as Node]
                kids.append(i)
                kids.append(_titled(title))
            } else { kids.append(_titled(n)) }
        }
        let d = _without({ $0.inList = false }) { SwiftUI._children(description, path + "/d") }
        if !d.isEmpty {
            var dn = _group(d, path + "/d")
            dn["modifiers"] = ((dn["modifiers"] as? [Node]) ?? []) + [["font": ["style": "title3"]] as Node,
                                                                      ["foregroundColor": ["semantic": "secondary", "hierarchical": true]] as Node,
                                                                      ["multilineTextAlignment": "center"] as Node,
                                                                      ["padding": ["top": _unavailable.descriptionGap, "leading": 0.0, "bottom": 0.0, "trailing": 0.0]] as Node]
            kids.append(dn)
        }
        let savedStyle = rt.buttonStyle
        rt.buttonStyle = "automatic"
        let a = _without({ $0.inList = false }) { SwiftUI._children(actions, path + "/a") }
        rt.buttonStyle = savedStyle
        if !a.isEmpty {
            var an = ["kind": "vstack", "id": path + "/a", "spacing": 8.0, "alignment": "center", "children": a] as Node
            an["modifiers"] = [["font": ["style": "subheadline"]] as Node,
                               ["padding": ["top": _unavailable.actionsGap - 6, "leading": 0.0, "bottom": 0.0, "trailing": 0.0]] as Node]
            kids.append(an)
        } else if !kids.isEmpty {
            // Without actions the parts still sit as high as if a row of actions followed them.
            let last = kids.count - 1
            let mods: [Node] = (kids[last]["modifiers"] as? [Node]) ?? []
            kids[last]["modifiers"] = mods + [["padding": ["top": 0.0, "leading": 0.0, "bottom": _unavailable.noActions, "trailing": 0.0] as Node] as Node]
        }
        // The first gap (symbol to title) differs from the others; spacing 6 between the texts.
        if kids.count > 1, kids[0]["kind"] as? String == "image" {
            let pad: Node = ["padding": ["top": _unavailable.titleGap - 6, "leading": 0.0, "bottom": 0.0, "trailing": 0.0] as Node]
            let mods: [Node] = (kids[1]["modifiers"] as? [Node]) ?? []
            kids[1]["modifiers"] = mods + [pad]
        }
        return ["kind": "vstack", "id": path, "spacing": 6.0, "alignment": "center", "children": kids,
                "modifiers": [["padding": ["top": 0.0, "leading": _unavailable.pad, "bottom": 0.0, "trailing": _unavailable.pad]] as Node,
                              ["frame": ["maxWidth": "inf", "maxHeight": "inf", "alignment": "center"]] as Node] as [Node]]
    }
}

func _titled(_ n: Node) -> Node {
    var t = n
    t["modifiers"] = ((t["modifiers"] as? [Node]) ?? []) + [["font": ["style": "title3", "weight": "bold"]] as Node, ["multilineTextAlignment": "center"] as Node]
    return t
}

extension ContentUnavailableView where Label == SwiftUI.Label<Text, Image>, Description == Text?, Actions == EmptyView {
    public init(_ title: String, systemImage name: String, description: Text? = nil) {
        self.init(label: { SwiftUI.Label(title, systemImage: name) }, description: { description })
    }
}
public struct SearchUnavailableContent {
    public struct Label: View {
        let query: String?
        @Environment(\.searchQuery) private var inheritedQuery

        init(query: String?) { self.query = query }

        public var body: some View {
            let whitespace: (Character) -> Bool = { $0.isWhitespace }
            let search = query ?? inheritedQuery
            let trimmed = String(search.drop(while: whitespace).reversed().drop(while: whitespace).reversed())
            let title = trimmed.isEmpty ? "No Results" : "No Results for \u{201C}\(trimmed)\u{201D}"
            return SwiftUI.Label(title, systemImage: "magnifyingglass")
        }
    }

    public struct Description: View {
        public var body: some View { Text("Check the spelling or try a new search.") }
    }

    public struct Actions: View {
        public var body: some View { EmptyView() }
    }
}

extension ContentUnavailableView where Label == SearchUnavailableContent.Label,
                                       Description == SearchUnavailableContent.Description,
                                       Actions == SearchUnavailableContent.Actions {
    public static var search: Self { search(query: nil) }
    public static func search(text: String) -> Self { search(query: text) }

    private static func search(query: String?) -> Self {
        Self(label: { Label(query: query) }, description: { Description() }, actions: { Actions() })
    }
}

// MARK: - Table

/// One table column: its title and how a row's cell is drawn.
public struct TableColumn<RowValue, Sort, Content: View, Label: View>: _TableColumns {
    let title: String
    let cell: (RowValue) -> Content
    public var _columns: [_TableColumn<RowValue>] { [_TableColumn(title: title) { v, p in SwiftUI._children(cell(v), p) }] }
}
extension TableColumn where Label == Text, Sort == Never {
    public init(_ title: String, @ViewBuilder content: @escaping (RowValue) -> Content) { self.title = title; cell = content }
}
extension TableColumn where Label == Text, Sort == Never, Content == Text {
    public init(_ title: String, value: KeyPath<RowValue, String>) { self.title = title; cell = { Text($0[keyPath: value]) } }
}
extension TableColumn where Label == Text, Content == Text {
    /// Sorting needs column headers, which compact tables do not show: the comparator is kept for API parity only.
    public init(_ title: String, value: KeyPath<RowValue, String>, comparator: Sort) { self.title = title; cell = { Text($0[keyPath: value]) } }
}
public protocol TableColumnContent {
    associatedtype TableRowValue
}
public struct _TableColumn<Row> {
    let title: String
    let cell: (Row, String) -> [Node]
}
public protocol _TableColumns {
    associatedtype _Row
    var _columns: [_TableColumn<_Row>] { get }
}
extension TableColumn: TableColumnContent { public typealias TableRowValue = RowValue }
public struct _TupleTableColumns<Row>: _TableColumns, TableColumnContent {
    public typealias TableRowValue = Row
    public let _columns: [_TableColumn<Row>]
}

@resultBuilder
public struct TableColumnBuilder<RowValue, Sort> {
    public static func buildExpression<C: _TableColumns>(_ c: C) -> _TupleTableColumns<RowValue> where C._Row == RowValue {
        _TupleTableColumns(_columns: c._columns)
    }
    public static func buildBlock(_ cs: _TupleTableColumns<RowValue>...) -> _TupleTableColumns<RowValue> {
        _TupleTableColumns(_columns: cs.flatMap { $0._columns })
    }
    public static func buildOptional(_ c: _TupleTableColumns<RowValue>?) -> _TupleTableColumns<RowValue> { c ?? _TupleTableColumns(_columns: []) }
    public static func buildEither(first c: _TupleTableColumns<RowValue>) -> _TupleTableColumns<RowValue> { c }
    public static func buildEither(second c: _TupleTableColumns<RowValue>) -> _TupleTableColumns<RowValue> { c }
}

/// Rows of values in columns. In a compact width (every iPhone) a table shows its first column as a plain list.
public struct Table<Value: Identifiable, Columns>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let rows: [Value], columns: [_TableColumn<Value>]
    public init<Data: RandomAccessCollection>(_ data: Data, @TableColumnBuilder<Value, Never> columns: () -> _TupleTableColumns<Value>)
    where Data.Element == Value, Columns == _TupleTableColumns<Value> {
        rows = Array(data); self.columns = columns()._columns
    }
    func _emit(_ path: String) -> Node {
        let (rows, columns) = (rows, columns)
        guard let first = columns.first else { return _list(path, style: "plain") { [] } }
        return _list(path, style: "plain") {
            rows.flatMap { r in first.cell(r, "\(path)/\(r.id)") }
        }
    }
}

/// `onDelete` / `onMove` handlers in scope for the ForEach being evaluated.
struct _RowActions {
    var delete: ((IndexSet) -> Void)?
    var move: ((IndexSet, Int) -> Void)?
}

/// An adaptive color (0...255 components).
func _rgb(_ l: (Double, Double, Double), dark d: (Double, Double, Double)) -> Node {
    ["r": l.0 / 255, "g": l.1 / 255, "b": l.2 / 255, "a": 1.0, "dark": [d.0 / 255, d.1 / 255, d.2 / 255]]
}

// MARK: - Collection editing (SwiftUI's IndexSet helpers for onDelete / onMove)

extension RangeReplaceableCollection where Self: MutableCollection {
    /// Removes the elements at `offsets` (positions from the start).
    public mutating func remove(atOffsets offsets: IndexSet) {
        for o in offsets.reversed() where o >= 0 && o < count { remove(at: index(startIndex, offsetBy: o)) }
    }
}

extension MutableCollection {
    /// Moves the elements at `source` so they sit before the element that was at `destination` (or at the end).
    public mutating func move(fromOffsets source: IndexSet, toOffset destination: Int) {
        let all = Array(self)
        let picked = source.filter { $0 >= 0 && $0 < all.count }
        guard !picked.isEmpty else { return }
        let dest = Swift.min(Swift.max(destination, 0), all.count)
        let before = all.enumerated().filter { !source.contains($0.offset) && $0.offset < dest }.map(\.element)
        let after = all.enumerated().filter { !source.contains($0.offset) && $0.offset >= dest }.map(\.element)
        let moved = picked.map { all[$0] }
        for (i, e) in zip(indices, before + moved + after) { self[i] = e }
    }
}
/// Disclosure chevron point size (semibold), iOS 27.2 swiftui-forms forms / forms-expanded.
let _disclosureChevron = 14.0
/// Both chevrons are centred in chevron.right's frame.
let _disclosureChevronFrame = 10.33

/// Edit-mode row controls (iOS 27.2, swiftui-forms edit-active): delete control and reorder handle point sizes, and
/// the gap between the delete control and the row.
let _editControl = (minus: 22.67, disc: 21.33, bar: (10.67, 2.0), handle: 21.67, gap: 17.67)
