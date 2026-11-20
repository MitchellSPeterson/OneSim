#if os(WASI)
import WASILibc
#else
import Foundation
#endif
internal import OneSimBridge

public protocol View {
    associatedtype Body: View
    @ViewBuilder var body: Body { get }
}

extension Never: View {
    public typealias Body = Never
    public var body: Never { fatalError("Never has no body") }
}

typealias Node = [String: Any]

/// Leaf views that emit a ViewNode directly instead of evaluating `body`.
protocol _Primitive { func _emit(_ path: String) -> Node }
/// Views that expand into several sibling nodes (tuples, optionals, conditionals).
protocol _Flattenable { func _children(_ path: String) -> [Node] }

/// Expand any view into sibling nodes; composites bind @State then recurse into body.
func _children<V: View>(_ v: V, _ path: String) -> [Node] {
    if let p = v as? _Primitive { return [p._emit(path)] }
    if let f = v as? _Flattenable { return f._children(path) }
    _bindState(v, path)
    return _children(v.body, path)
}

/// Evaluate to exactly one node (several siblings are wrapped in a vstack).
func _eval<V: View>(_ v: V, _ path: String) -> Node {
    let c = _children(v, path)
    if c.count == 1 { return c[0] }
    if c.isEmpty { return ["kind": "empty", "id": path] }
    return ["kind": "vstack", "id": path, "spacing": 0.0, "alignment": "center", "children": c]
}

@resultBuilder
public struct ViewBuilder {
    public static func buildBlock() -> EmptyView { EmptyView() }
    public static func buildBlock<C: View>(_ c: C) -> C { c }
    public static func buildExpression<C: View>(_ c: C) -> C { c }
    public static func buildOptional<C: View>(_ c: C?) -> C? { c }
    public static func buildEither<T: View, F: View>(first: T) -> _ConditionalContent<T, F> { .init(.first(first)) }
    public static func buildEither<T: View, F: View>(second: F) -> _ConditionalContent<T, F> { .init(.second(second)) }
    public static func buildLimitedAvailability<C: View>(_ c: C) -> C { c }
    public static func buildBlock<A: View, B: View>(_ a: A, _ b: B) -> TupleView<(A, B)> { .init((a, b)) }
    public static func buildBlock<A: View, B: View, C: View>(_ a: A, _ b: B, _ c: C) -> TupleView<(A, B, C)> { .init((a, b, c)) }
    public static func buildBlock<A: View, B: View, C: View, D: View>(_ a: A, _ b: B, _ c: C, _ d: D) -> TupleView<(A, B, C, D)> { .init((a, b, c, d)) }
    public static func buildBlock<A: View, B: View, C: View, D: View, E: View>(_ a: A, _ b: B, _ c: C, _ d: D, _ e: E) -> TupleView<(A, B, C, D, E)> { .init((a, b, c, d, e)) }
    public static func buildBlock<A: View, B: View, C: View, D: View, E: View, F: View>(_ a: A, _ b: B, _ c: C, _ d: D, _ e: E, _ f: F) -> TupleView<(A, B, C, D, E, F)> { .init((a, b, c, d, e, f)) }
    public static func buildBlock<A: View, B: View, C: View, D: View, E: View, F: View, G: View>(_ a: A, _ b: B, _ c: C, _ d: D, _ e: E, _ f: F, _ g: G) -> TupleView<(A, B, C, D, E, F, G)> { .init((a, b, c, d, e, f, g)) }
    public static func buildBlock<A: View, B: View, C: View, D: View, E: View, F: View, G: View, H: View>(_ a: A, _ b: B, _ c: C, _ d: D, _ e: E, _ f: F, _ g: G, _ h: H) -> TupleView<(A, B, C, D, E, F, G, H)> { .init((a, b, c, d, e, f, g, h)) }
    public static func buildBlock<A: View, B: View, C: View, D: View, E: View, F: View, G: View, H: View, I: View>(_ a: A, _ b: B, _ c: C, _ d: D, _ e: E, _ f: F, _ g: G, _ h: H, _ i: I) -> TupleView<(A, B, C, D, E, F, G, H, I)> { .init((a, b, c, d, e, f, g, h, i)) }
    public static func buildBlock<A: View, B: View, C: View, D: View, E: View, F: View, G: View, H: View, I: View, J: View>(_ a: A, _ b: B, _ c: C, _ d: D, _ e: E, _ f: F, _ g: G, _ h: H, _ i: I, _ j: J) -> TupleView<(A, B, C, D, E, F, G, H, I, J)> { .init((a, b, c, d, e, f, g, h, i, j)) }
}

public struct TupleView<T>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let value: T
    init(_ v: T) { value = v }
    func _children(_ path: String) -> [Node] {
        var out: [Node] = []
        for (i, ch) in Mirror(reflecting: value).children.enumerated() {
            if let v = ch.value as? any View { out += SwiftUI._children(v, "\(path)/\(i)") }
        }
        return out
    }
}

public struct EmptyView: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    public init() {}
    func _children(_ path: String) -> [Node] { [] }
}

extension Optional: View where Wrapped: View {
    public typealias Body = Never
    public var body: Never { fatalError() }
}
extension Optional: _Flattenable where Wrapped: View {
    func _children(_ path: String) -> [Node] { map { SwiftUI._children($0, path) } ?? [] }
}

public struct _ConditionalContent<T: View, F: View>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    enum Storage { case first(T), second(F) }
    let storage: Storage
    init(_ s: Storage) { storage = s }
    func _children(_ path: String) -> [Node] {
        switch storage {
        case .first(let v): return SwiftUI._children(v, path)
        case .second(let v): return SwiftUI._children(v, path)
        }
    }
}

// MARK: - Runtime

/// Global (single-threaded) runtime: state registry, action table, version, environment.
final class _Runtime {
    nonisolated(unsafe) static let shared = _Runtime()
    var slots: [String: AnyObject] = [:]
    var touched = Set<String>()
    var actions: [UInt64: () -> Void] = [:]
    var nextAction: UInt64 = 1
    var version: UInt64 = 0
    var dirty = false
    var appearance = "light"
    var viewport = (width: 393.0, height: 852.0)
    var render: () -> Node = { [:] }
    // Per-evaluation context (reset every submit; set while walking modifier subtrees).
    var fields: [UInt64: (String) -> Void] = [:]
    var disabled = false
    var buttonStyle = "automatic"
    var pickerStyle = "menu"
    var tags: [String: AnyHashable] = [:]   // `.tag(_:)` values by node id, read by `Picker`
    var nav: _NavState?
    var envObjects: [ObjectIdentifier: AnyObject] = [:]
    // Presentation layer: set by .alert / .sheet while evaluating; hoisted above the tree by `_evaluate`.
    var layers: [Node] = []          // alerts, dialogs, sheets, covers, bottom to top (a sheet reserves its slot before its content)
    var autoDismiss: (() -> Void)?   // runs after any Button in an alert's actions
    var dismissHook: (() -> Void)?   // `@Environment(\.dismiss)` inside a sheet
    var dynamicType = DynamicTypeSize.large   // from SIMCORE_DYNAMIC_TYPE at launch; the env callback carries no live updates
    // onAppear/onDisappear: keys seen in this evaluation vs. the previous one (value = that key's onDisappear).
    var lifecycleNow: [String: (appear: () -> Void, disappear: () -> Void)] = [:]
    var lifecycleOrder: [String] = []
    var appeared: [String: () -> Void] = [:]
    // onChange: last value seen per structural path (committed at the end of each pass) and the callbacks this pass queued.
    var changeSeen: [String: Any] = [:], changeNow: [String: Any] = [:]
    var changePending: [() -> Void] = []
    // Gestures (section L): registered by this evaluation by key; `gesturesOff` while a `GestureMask` excludes subviews.
    var gestures: [String: _GestureEntry] = [:]
    var gesturesOff = false
    var touch: _TouchSession?   // the touch sequence in progress (spans evaluations)
    // Custom shapes (section N): laid-out sizes sent by the host via `layout` messages, keyed by node id.
    var shapeSizes: [String: CGSize] = [:]
    // Layout direction for `Shape.layoutDirectionBehavior`.
    var layoutDirection: LayoutDirection = .leftToRight
    // Layout (section M). Frames and the last scroll-to key survive a pass; the stacks are rebuilt each pass.
    var geometries: [String: _Geo] = [:]
    var safeTop = 0.0, safeBottom = 0.0   // SIMCORE_INSETS at launch ("top,bottom"); leading/trailing stay 0
    var scrollStack: [String] = []
    var anchorScrollNow: [String: String] = [:]
    var anchorScroll: [String: String] = [:]
    var scrolledTo: [String: String?] = [:]
    var scrollPos: [String: (String?) -> Void] = [:]
    #if !os(WASI)
    /// `AsyncImage` HTTP loads. Nil uses `URLSession.shared`. Set by `.asyncImageURLSession`.
    var imageSession: URLSession?
    #endif

    /// Compare `value` with the previous pass's; queue `fire(old, new)` on a change (or on first sight when `initial`).
    func noteChange<V: Equatable>(_ path: String, _ value: V, initial: Bool, _ fire: @escaping (V, V) -> Void) {
        var k = path
        while changeNow[k] != nil { k += "'" }   // several onChange on one view
        changeNow[k] = value
        if let old = changeSeen[k] as? V { if old != value { changePending.append { fire(old, value) } } }
        else if initial { changePending.append { fire(value, value) } }
    }

    func noteLifecycle(_ path: String, _ appear: (() -> Void)?, _ disappear: (() -> Void)?) {
        var k = path
        while lifecycleNow[k] != nil { k += "'" }   // several modifiers on one view
        lifecycleNow[k] = (appear ?? {}, disappear ?? {})
        lifecycleOrder.append(k)
    }

    /// Fire onDisappear for keys that vanished, then onAppear for new ones.
    private func _lifecycle() {
        let now = lifecycleNow, prev = appeared
        appeared = now.mapValues { $0.disappear }
        for k in prev.keys.sorted() where now[k] == nil { prev[k]?() }
        for k in lifecycleOrder where prev[k] == nil { now[k]?.appear() }
    }

    func registerField(_ f: @escaping (String) -> Void) -> UInt64 {
        defer { nextAction += 1 }
        fields[nextAction] = f
        return nextAction
    }

    func registerAction(_ f: @escaping () -> Void) -> UInt64 {
        defer { nextAction += 1 }
        actions[nextAction] = f
        return nextAction
    }

    /// Re-evaluate the whole root, drop stale actions/state, and submit.
    func submitTree() { _pool { _submitTree() } }

    private func _evaluate() -> Node {
        actions.removeAll(keepingCapacity: true)
        fields.removeAll(keepingCapacity: true)
        disabled = false; buttonStyle = "automatic"; pickerStyle = "menu"; tags = [:]; nav = nil; envObjects = [:]
        #if !os(WASI)
        imageSession = nil
        #endif
        layers = []; autoDismiss = nil; dismissHook = nil
        lifecycleNow = [:]; lifecycleOrder = []; changeNow = [:]; changePending = []
        gestures = [:]; gesturesOff = false
        scrollStack.removeAll(keepingCapacity: true)
        anchorScrollNow.removeAll(keepingCapacity: true)
        scrollPos.removeAll(keepingCapacity: true)
        touched.removeAll(keepingCapacity: true)
        var tree = render()
        if !layers.isEmpty { tree = ["kind": "presentation", "id": "presentation", "content": tree, "layers": layers] }
        slots = slots.filter { touched.contains($0.key) }
        changeSeen = changeNow
        anchorScroll = anchorScrollNow   // a later button action (stack empty) can still scrollTo
        dirty = false
        return tree
    }

    private func _submitTree() {
        var tree = _evaluate()
        // Lifecycle callbacks run after the pass; if they dirtied state, re-evaluate (capped against onAppear loops).
        for _ in 0..<8 {
            _lifecycle()
            let fire = changePending; changePending = []
            fire.forEach { $0() }
            guard dirty else { break }
            tree = _evaluate()
        }
        dirty = false
        version += 1
        #if os(WASI)
        var json = ""
        _json(tree, &json)
        let rc = json.withCString { p in
            p.withMemoryRebound(to: UInt8.self, capacity: json.utf8.count) { simcore_submit_tree($0, json.utf8.count, version) }
        }
        #else
        guard let data = try? JSONSerialization.data(withJSONObject: tree, options: [.withoutEscapingSlashes]) else {
            "tree serialization failed".withCString { simcore_report_error($0) }
            return
        }
        let rc = data.withUnsafeBytes { simcore_submit_tree($0.bindMemory(to: UInt8.self).baseAddress, data.count, version) }
        #endif
        if rc != 0, let e = simcore_last_error() { simcore_report_error(e) }
    }
}

/// Drain Foundation's autoreleased bridging objects each cycle (no run loop here to do it).
func _pool(_ f: () -> Void) {
    #if canImport(ObjectiveC)
    autoreleasepool(invoking: f)
    #else
    f()
    #endif
}

#if os(WASI)
/// Entry point on WASI (reactor): registers the app and returns. The first render happens when
/// onesim-web pushes the environment (simcore_web_init); the browser is the event loop.
public func _onesimRun<V: View>(_ root: V) {
    let rt = _Runtime.shared
    rt.render = { _eval(root, "0") }
    _onesimRegisterCallbacks()
}
#else
/// Entry point used by `App.main()` and generated mains. Never returns.
public func _onesimRun<V: View>(_ root: V) -> Never {
    let rt = _Runtime.shared
    if simcore_runtime_init() != 0 {
        FileHandle.standardError.write(Data("onesim init failed: \(simcore_last_error().map { String(cString: $0) } ?? "?")\n".utf8))
        exit(1)
    }
    let env = ProcessInfo.processInfo.environment
    if let a = env["SIMCORE_APPEARANCE"] { rt.appearance = a }
    if let vp = env["SIMCORE_VIEWPORT"]?.split(separator: "x").compactMap({ Double($0) }), vp.count == 2 {
        rt.viewport = (vp[0], vp[1])
    }
    if let ins = env["SIMCORE_INSETS"]?.split(separator: ",").compactMap({ Double($0.trimmingCharacters(in: .whitespaces)) }), ins.count == 2 {
        rt.safeTop = ins[0]; rt.safeBottom = ins[1]
    }
    if let d = env["SIMCORE_DYNAMIC_TYPE"], let s = DynamicTypeSize.allCases.first(where: { "\($0)" == d }) { rt.dynamicType = s }
    rt.render = { _eval(root, "0") }
    _onesimRegisterCallbacks()
    rt.submitTree()
    exit(simcore_runtime_run())
}
#endif

func _onesimRegisterCallbacks() {
    simcore_runtime_set_callbacks(nil, { _, id in
        let rt = _Runtime.shared
        _pool { rt.actions[id]?() }
        if rt.dirty { rt.submitTree() }
    }, { _, appearance, w, h in
        let rt = _Runtime.shared
        if let a = appearance { rt.appearance = String(cString: a) }
        rt.viewport = (w, h)
        rt.submitTree()
    })
    simcore_runtime_set_gesture_callback { _, phase, time, pts, np, keys, frames, nt in
        guard let phase = _Touch.Phase(rawValue: phase) else { return }
        let points = (0..<np).map { CGPoint(x: pts![2 * $0], y: pts![2 * $0 + 1]) }
        let targets = (0..<nt).map { i -> (String, CGPoint, CGSize) in
            (String(cString: keys![i]!), CGPoint(x: frames![4 * i], y: frames![4 * i + 1]), CGSize(width: frames![4 * i + 2], height: frames![4 * i + 3]))
        }
        _handleTouch(phase, time, points, targets)
    }
    simcore_runtime_set_text_callback { _, field, text in
        let rt = _Runtime.shared
        if let text { let t = String(cString: text); _pool { rt.fields[field]?(t) } }
        if rt.dirty { rt.submitTree() }
    }
    simcore_runtime_set_layout_callback { _, line in
        guard let line else { return }
        let text = String(cString: line)
        let rt = _Runtime.shared
        _pool { _applyLayout(text) }
        if rt.dirty { rt.submitTree() }
    }
}

/// One guest line (`scroll_to_id`). Key order does not matter.
func _send(_ msg: Node) {
    #if os(WASI)
    var json = ""
    _json(msg, &json)
    json.withCString { _ = simcore_send_line($0) }
    #else
    guard let data = try? JSONSerialization.data(withJSONObject: msg, options: [.withoutEscapingSlashes]),
          let s = String(data: data, encoding: .utf8) else { return }
    s.withCString { _ = simcore_send_line($0) }
    #endif
}

func _sendScroll(_ scroll: String, _ target: String, _ anchor: UnitPoint?) {
    var m: Node = ["type": "scroll_to_id", "scroll": scroll, "target": target]
    if let anchor { m["anchor"] = [Double(anchor.x), Double(anchor.y)] }
    _send(m)
}

func _jsonObject(_ line: String) -> [String: Any]? {
    #if os(WASI)
    return _jsonParse(line) as? [String: Any]
    #else
    (try? JSONSerialization.jsonObject(with: Data(line.utf8))) as? [String: Any]
    #endif
}

func _f64(_ v: Any?) -> Double {
    switch v {
    case let d as Double: return d
    case let i as Int: return Double(i)
    #if !os(WASI)
    case let n as NSNumber: return n.doubleValue
    #endif
    default: return 0
    }
}

/// Host layout line: shape or geometry frames, or a scroll position writeback. The scroll setter runs in the caller's pool.
func _applyLayout(_ line: String) {
    guard let obj = _jsonObject(line), let type = obj["type"] as? String else { return }
    let rt = _Runtime.shared
    if type == "layout" {
        guard let frames = obj["frames"] as? [String: Any] else { return }
        for (id, raw) in frames {
            guard let box = raw as? [Any], box.count == 4 else { continue }
            let size = CGSize(width: _f64(box[2]), height: _f64(box[3]))
            if rt.shapeSizes[id] != size { rt.shapeSizes[id] = size; rt.dirty = true }
        }
    } else if type == "geometry" {
        guard let frames = obj["frames"] as? [String: Any] else { return }
        for (id, raw) in frames {
            guard let box = raw as? [String: Any] else { continue }
            let g = _Geo(w: _f64(box["width"]), h: _f64(box["height"]), x: _f64(box["x"]), y: _f64(box["y"]),
                         safeTop: _f64(box["safeTop"]), safeBottom: _f64(box["safeBottom"]),
                         safeLeading: _f64(box["safeLeading"]), safeTrailing: _f64(box["safeTrailing"]))
            if rt.geometries[id] != g { rt.geometries[id] = g; rt.dirty = true }
        }
    } else if type == "scroll_position", let scroll = obj["scroll"] as? String {
        rt.scrollPos[scroll]?(obj["target"] as? String)   // NSNull target becomes nil
    }
}
