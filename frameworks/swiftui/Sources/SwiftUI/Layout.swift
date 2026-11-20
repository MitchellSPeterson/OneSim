#if !os(WASI)
import Foundation
import CoreGraphics
#endif

// Layout (protocol section M). Children of lazy stacks are built eagerly; the host does the fitting and snapping.

struct _Geo: Equatable {
    var w, h, x, y, safeTop, safeBottom, safeLeading, safeTrailing: Double
}

extension _Runtime {
    /// Size before any host `geometry` line: the viewport, with launch safe-area insets.
    func fallbackGeo() -> _Geo {
        _Geo(w: viewport.width, h: viewport.height, x: 0, y: 0, safeTop: safeTop, safeBottom: safeBottom, safeLeading: 0, safeTrailing: 0)
    }
}

public struct EdgeInsets: Equatable {
    public var top, leading, bottom, trailing: CGFloat
    public init(top: CGFloat = 0, leading: CGFloat = 0, bottom: CGFloat = 0, trailing: CGFloat = 0) {
        self.top = top; self.leading = leading; self.bottom = bottom; self.trailing = trailing
    }
}

public struct GeometryProxy {
    let geo: _Geo
    public var size: CGSize { CGSize(width: geo.w, height: geo.h) }
    public var safeAreaInsets: EdgeInsets {
        EdgeInsets(top: geo.safeTop, leading: geo.safeLeading, bottom: geo.safeBottom, trailing: geo.safeTrailing)
    }
    /// `.local` is the proposal at the origin. `.global` and a named space are the host frame (named == global).
    public func frame(in coordinateSpace: CoordinateSpace) -> CGRect {
        coordinateSpace == .local ? CGRect(x: 0, y: 0, width: geo.w, height: geo.h) : CGRect(x: geo.x, y: geo.y, width: geo.w, height: geo.h)
    }
}

public struct GeometryReader<Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let content: (GeometryProxy) -> Content
    public init(@ViewBuilder content: @escaping (GeometryProxy) -> Content) { self.content = content }
    func _emit(_ path: String) -> Node {
        let g = _Runtime.shared.geometries[path] ?? _Runtime.shared.fallbackGeo()
        return ["kind": "geometry", "id": path, "proposal": [g.w, g.h], "child": _eval(content(GeometryProxy(geo: g)), path + "/0")]
    }
}

// MARK: - Scroll identity

public protocol _ScrollID {
    var scrollKey: String { get }
    static func fromScrollKey(_ key: String) -> Self?
}
extension String: _ScrollID {
    public var scrollKey: String { self }
    public static func fromScrollKey(_ key: String) -> String? { key }
}
extension Int: _ScrollID {
    public var scrollKey: String { String(self) }
    public static func fromScrollKey(_ key: String) -> Int? { Int(key) }
}

func _idKey<ID: Hashable>(_ id: ID) -> String {
    if let s = id as? String { return s }
    if let i = id as? Int { return String(i) }
    return String(describing: id)
}

public struct ScrollViewProxy {
    public func scrollTo<ID: Hashable>(_ id: ID, anchor: UnitPoint? = nil) {
        let key = _idKey(id)
        guard let scroll = _Runtime.shared.anchorScroll[key] else { return }
        _sendScroll(scroll, key, anchor)
    }
}

public struct ScrollViewReader<Content: View>: View {
    let content: (ScrollViewProxy) -> Content
    public init(@ViewBuilder content: @escaping (ScrollViewProxy) -> Content) { self.content = content }
    public var body: some View { content(ScrollViewProxy()) }
}

// MARK: - Scroll targets

public struct ScrollTarget: Equatable {
    public var rect: CGRect
    public var anchor: UnitPoint?
    public init(rect: CGRect = .zero, anchor: UnitPoint? = nil) { self.rect = rect; self.anchor = anchor }
}

public struct ScrollTargetBehaviorContext {
    public var containerSize: CGSize
    public var axes: Axis.Set
    public init(containerSize: CGSize = .zero, axes: Axis.Set = [.vertical, .horizontal]) {
        self.containerSize = containerSize; self.axes = axes
    }
}

public protocol ScrollTargetBehavior {
    func updateTarget(_ target: inout ScrollTarget, context: ScrollTargetBehaviorContext)
}

public struct PagingScrollTargetBehavior: ScrollTargetBehavior {
    public init() {}
    public func updateTarget(_ target: inout ScrollTarget, context: ScrollTargetBehaviorContext) {}
}
extension ScrollTargetBehavior where Self == PagingScrollTargetBehavior {
    public static var paging: PagingScrollTargetBehavior { PagingScrollTargetBehavior() }
}

public struct ViewAlignedScrollTargetBehavior: ScrollTargetBehavior {
    public enum LimitBehavior: Hashable { case automatic, always, never }
    public var limitBehavior: LimitBehavior
    public init(limitBehavior: LimitBehavior = .automatic) { self.limitBehavior = limitBehavior }
    public func updateTarget(_ target: inout ScrollTarget, context: ScrollTargetBehaviorContext) {}
}
extension ScrollTargetBehavior where Self == ViewAlignedScrollTargetBehavior {
    public static var viewAligned: ViewAlignedScrollTargetBehavior { ViewAlignedScrollTargetBehavior() }
    public static func viewAligned(limitBehavior: ViewAlignedScrollTargetBehavior.LimitBehavior) -> ViewAlignedScrollTargetBehavior {
        ViewAlignedScrollTargetBehavior(limitBehavior: limitBehavior)
    }
}

public struct AnyScrollTargetBehavior: ScrollTargetBehavior {
    let update: (inout ScrollTarget, ScrollTargetBehaviorContext) -> Void
    public init<B: ScrollTargetBehavior>(_ behavior: B) {
        update = { target, context in behavior.updateTarget(&target, context: context) }
    }
    public func updateTarget(_ target: inout ScrollTarget, context: ScrollTargetBehaviorContext) { update(&target, context) }
}

func _snapName<B: ScrollTargetBehavior>(_ behavior: B) -> String? {
    if behavior is PagingScrollTargetBehavior { return "paging" }
    if behavior is ViewAlignedScrollTargetBehavior { return "viewAligned" }
    return nil
}

/// No behavior key when `name` is nil (a custom `ScrollTargetBehavior`; its `updateTarget` is not called).
struct _BehaviorMod<Content: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content
    let name: String?
    func _children(_ path: String) -> [Node] {
        let nodes = _modTargets(content, path)
        guard let name else { return nodes }
        return nodes.map { var n = $0; n["modifiers"] = ((n["modifiers"] as? [Node]) ?? []) + [["scrollTargetBehavior": name]]; return n }
    }
}

public enum ScrollIndicatorVisibility: Hashable { case automatic, visible, hidden }

public enum ContentMarginPlacement: Hashable { case automatic, scrollContent, scrollIndicators }

extension View {
    public func id<ID: Hashable>(_ id: ID) -> some View {
        let key = _idKey(id)
        return _modifiedAt { _ in
            let rt = _Runtime.shared
            if let scroll = rt.scrollStack.last { rt.anchorScrollNow[key] = scroll }
            return ["id": key]
        }
    }

    public func scrollPosition<ID: _ScrollID>(id: Binding<ID?>) -> some View {
        let binding = id
        return _modifiedAt { path in
            let rt = _Runtime.shared
            rt.scrollPos[path] = { key in
                rt.scrolledTo[path] = key   // before the binding, so the re-eval does not echo scroll_to_id
                binding.wrappedValue = key.flatMap(ID.fromScrollKey)
            }
            let key = binding.wrappedValue?.scrollKey
            if rt.scrolledTo[path] != key {
                rt.scrolledTo[path] = key
                if let key { _sendScroll(path, key, nil) }
            }
            return ["scrollPosition": NSNull()]
        }
    }

    public func scrollTargetBehavior<B: ScrollTargetBehavior>(_ behavior: B) -> some View {
        _BehaviorMod(content: self, name: _snapName(behavior))
    }
    public func scrollTargetLayout(isEnabled: Bool = true) -> some View { _modified { ["scrollTargetLayout": isEnabled] } }

    public func scrollIndicators(_ visibility: ScrollIndicatorVisibility, axes: Axis.Set = [.vertical, .horizontal]) -> some View {
        let name = visibility == .automatic ? "automatic" : visibility == .visible ? "visible" : "hidden"
        var box: Node = [:]
        if axes.contains(.horizontal) { box["horizontal"] = name }
        if axes.contains(.vertical) { box["vertical"] = name }
        return _modified { ["scrollIndicators": box] }
    }

    /// `placement` is accepted and ignored: the length is applied to `edges` either way.
    public func contentMargins(_ length: CGFloat, for placement: ContentMarginPlacement = .automatic) -> some View {
        contentMargins(.all, length, for: placement)
    }
    public func contentMargins(_ edges: Edge.Set, _ length: CGFloat?, for _: ContentMarginPlacement = .automatic) -> some View {
        let l = Double(length ?? 0)   // nil length -> zero insets
        return _modified { ["contentMargins": ["top": edges.contains(.top) ? l : 0, "leading": edges.contains(.leading) ? l : 0,
                                                "bottom": edges.contains(.bottom) ? l : 0, "trailing": edges.contains(.trailing) ? l : 0]] }
    }

    public func gridCellColumns(_ count: Int) -> some View { _modified { ["gridCellColumns": count] } }
}

// MARK: - Lazy stacks

/// A `_section` node becomes a text header (pinned when asked) followed by its rows. Footers are only a stack flag.
func _lazyKids(_ nodes: [Node], _ path: String, _ pinHeaders: Bool) -> [Node] {
    var out: [Node] = []
    var s = 0
    for n in nodes {
        guard (n["kind"] as? String) == "_section" else { out.append(n); continue }
        if let header = n["header"] as? String {
            var t: Node = ["kind": "text", "id": "\(path)/s\(s)", "text": header]
            if pinHeaders { t["modifiers"] = [["pin": "header"] as Node] }
            out.append(t)
        }
        out += (n["rows"] as? [Node]) ?? []
        s += 1
    }
    return out
}

func _pinMode(_ pinned: PinnedScrollableViews) -> String? {
    switch (pinned.contains(.sectionHeaders), pinned.contains(.sectionFooters)) {
    case (true, true): return "both"
    case (true, false): return "headers"
    case (false, true): return "footers"
    default: return nil
    }
}

public struct LazyVStack<Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let alignment: HorizontalAlignment, spacing: CGFloat?, pinned: PinnedScrollableViews, content: Content
    public init(alignment: HorizontalAlignment = .center, spacing: CGFloat? = nil, pinnedViews: PinnedScrollableViews = [], @ViewBuilder content: () -> Content) {
        self.alignment = alignment; self.spacing = spacing; self.pinned = pinnedViews; self.content = content()
    }
    func _emit(_ path: String) -> Node {
        var n: Node = ["kind": "vstack", "id": path, "alignment": alignment.name, "children": _lazyKids(_children(content, path), path, pinned.contains(.sectionHeaders))]
        if let spacing { n["spacing"] = _num(spacing) }
        if let pin = _pinMode(pinned) { n["pinned"] = pin }
        return n
    }
}

public struct LazyHStack<Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let alignment: VerticalAlignment, spacing: CGFloat?, pinned: PinnedScrollableViews, content: Content
    public init(alignment: VerticalAlignment = .center, spacing: CGFloat? = nil, pinnedViews: PinnedScrollableViews = [], @ViewBuilder content: () -> Content) {
        self.alignment = alignment; self.spacing = spacing; self.pinned = pinnedViews; self.content = content()
    }
    func _emit(_ path: String) -> Node {
        var n: Node = ["kind": "hstack", "id": path, "alignment": alignment.name, "children": _lazyKids(_children(content, path), path, pinned.contains(.sectionHeaders))]
        if let spacing { n["spacing"] = _num(spacing) }
        if let pin = _pinMode(pinned) { n["pinned"] = pin }
        return n
    }
}

// MARK: - Grid

public struct GridItem {
    public enum Size {
        case fixed(CGFloat)
        case flexible(minimum: CGFloat = 10, maximum: CGFloat = .infinity)
        case adaptive(minimum: CGFloat, maximum: CGFloat = .infinity)
    }
    public var size: Size
    public var spacing: CGFloat?
    public var alignment: Alignment?
    public init(_ size: Size = .flexible(), spacing: CGFloat? = nil, alignment: Alignment? = nil) {
        self.size = size; self.spacing = spacing; self.alignment = alignment
    }
    var json: Node {
        var n = Node()
        switch size {
        case .fixed(let v): n["size"] = "fixed"; n["value"] = _num(v)
        case .flexible(let min, let max): n["size"] = "flexible"; n["min"] = _num(min); n["max"] = _num(max)
        case .adaptive(let min, let max): n["size"] = "adaptive"; n["min"] = _num(min); n["max"] = _num(max)
        }
        if let spacing { n["spacing"] = _num(spacing) }
        if let alignment { n["alignment"] = alignment.name }
        return n
    }
}

public struct GridRow<Content: View>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let alignment: VerticalAlignment?, content: Content
    public init(alignment: VerticalAlignment? = nil, @ViewBuilder content: () -> Content) {
        self.alignment = alignment; self.content = content()
    }
    func _children(_ path: String) -> [Node] {
        var n: Node = ["kind": "_gridrow", "cells": SwiftUI._children(content, path)]
        if let alignment { n["alignment"] = alignment.name }
        return [n]
    }
}

public struct Grid<Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let alignment: Alignment, horizontalSpacing: CGFloat?, verticalSpacing: CGFloat?, content: Content
    public init(alignment: Alignment = .center, horizontalSpacing: CGFloat? = nil, verticalSpacing: CGFloat? = nil, @ViewBuilder content: () -> Content) {
        self.alignment = alignment; self.horizontalSpacing = horizontalSpacing; self.verticalSpacing = verticalSpacing; self.content = content()
    }
    func _emit(_ path: String) -> Node {
        var rows: [Node] = []
        for n in _children(content, path) {
            if (n["kind"] as? String) == "_gridrow" {
                var row = n; row["kind"] = nil; rows.append(row)
            } else { rows.append(["cells": [n]]) }
        }
        var node: Node = ["kind": "grid", "id": path, "alignment": alignment.name, "rows": rows]
        if let horizontalSpacing { node["horizontalSpacing"] = _num(horizontalSpacing) }
        if let verticalSpacing { node["verticalSpacing"] = _num(verticalSpacing) }
        return node
    }
}

public struct LazyVGrid<Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let tracks: [GridItem], alignment: HorizontalAlignment, spacing: CGFloat?, content: Content
    public init(columns: [GridItem], alignment: HorizontalAlignment = .center, spacing: CGFloat? = nil, pinnedViews _: PinnedScrollableViews = [], @ViewBuilder content: () -> Content) {
        self.tracks = columns; self.alignment = alignment; self.spacing = spacing; self.content = content()
    }
    func _emit(_ path: String) -> Node { _lazyGrid("vertical", alignment.name, tracks, spacing, content, path) }
}

public struct LazyHGrid<Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let tracks: [GridItem], alignment: VerticalAlignment, spacing: CGFloat?, content: Content
    public init(rows: [GridItem], alignment: VerticalAlignment = .center, spacing: CGFloat? = nil, pinnedViews _: PinnedScrollableViews = [], @ViewBuilder content: () -> Content) {
        self.tracks = rows; self.alignment = alignment; self.spacing = spacing; self.content = content()
    }
    func _emit(_ path: String) -> Node { _lazyGrid("horizontal", alignment.name, tracks, spacing, content, path) }
}

func _lazyGrid<Content: View>(_ axis: String, _ alignment: String, _ tracks: [GridItem], _ spacing: CGFloat?, _ content: Content, _ path: String) -> Node {
    var n: Node = ["kind": "lazyGrid", "id": path, "axis": axis, "alignment": alignment, "tracks": tracks.map { $0.json }, "children": _children(content, path)]
    if let spacing { n["spacing"] = _num(spacing) }
    return n
}

public struct ViewThatFits<Content: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let axes: Axis.Set, content: Content
    public init(in axes: Axis.Set = [.horizontal, .vertical], @ViewBuilder content: () -> Content) {
        self.axes = axes; self.content = content()
    }
    func _emit(_ path: String) -> Node {
        let h = axes.contains(.horizontal), v = axes.contains(.vertical)
        let name = (h && v) || axes.isEmpty ? "both" : h ? "horizontal" : "vertical"
        return ["kind": "viewThatFits", "id": path, "axes": name, "children": _children(content, path + "/0")]
    }
}
