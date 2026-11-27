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
    public static func buildBlock<A: View, B: View>(_ a: A, _ b: B) -> TupleView<(A, B)> {
        .init((a, b)) { path in
            var out: [Node] = []
            out.reserveCapacity(2)
            out += SwiftUI._children(a, "\(path)/0")
            out += SwiftUI._children(b, "\(path)/1")
            return out
        }
    }
    public static func buildBlock<A: View, B: View, C: View>(_ a: A, _ b: B, _ c: C) -> TupleView<(A, B, C)> {
        .init((a, b, c)) { path in
            var out: [Node] = []
            out.reserveCapacity(3)
            out += SwiftUI._children(a, "\(path)/0")
            out += SwiftUI._children(b, "\(path)/1")
            out += SwiftUI._children(c, "\(path)/2")
            return out
        }
    }
    public static func buildBlock<A: View, B: View, C: View, D: View>(_ a: A, _ b: B, _ c: C, _ d: D) -> TupleView<(A, B, C, D)> {
        .init((a, b, c, d)) { path in
            var out: [Node] = []
            out.reserveCapacity(4)
            out += SwiftUI._children(a, "\(path)/0")
            out += SwiftUI._children(b, "\(path)/1")
            out += SwiftUI._children(c, "\(path)/2")
            out += SwiftUI._children(d, "\(path)/3")
            return out
        }
    }
    public static func buildBlock<A: View, B: View, C: View, D: View, E: View>(_ a: A, _ b: B, _ c: C, _ d: D, _ e: E) -> TupleView<(A, B, C, D, E)> {
        .init((a, b, c, d, e)) { path in
            var out: [Node] = []
            out.reserveCapacity(5)
            out += SwiftUI._children(a, "\(path)/0")
            out += SwiftUI._children(b, "\(path)/1")
            out += SwiftUI._children(c, "\(path)/2")
            out += SwiftUI._children(d, "\(path)/3")
            out += SwiftUI._children(e, "\(path)/4")
            return out
        }
    }
    public static func buildBlock<A: View, B: View, C: View, D: View, E: View, F: View>(_ a: A, _ b: B, _ c: C, _ d: D, _ e: E, _ f: F) -> TupleView<(A, B, C, D, E, F)> {
        .init((a, b, c, d, e, f)) { path in
            var out: [Node] = []
            out.reserveCapacity(6)
            out += SwiftUI._children(a, "\(path)/0")
            out += SwiftUI._children(b, "\(path)/1")
            out += SwiftUI._children(c, "\(path)/2")
            out += SwiftUI._children(d, "\(path)/3")
            out += SwiftUI._children(e, "\(path)/4")
            out += SwiftUI._children(f, "\(path)/5")
            return out
        }
    }
    public static func buildBlock<A: View, B: View, C: View, D: View, E: View, F: View, G: View>(_ a: A, _ b: B, _ c: C, _ d: D, _ e: E, _ f: F, _ g: G) -> TupleView<(A, B, C, D, E, F, G)> {
        .init((a, b, c, d, e, f, g)) { path in
            var out: [Node] = []
            out.reserveCapacity(7)
            out += SwiftUI._children(a, "\(path)/0")
            out += SwiftUI._children(b, "\(path)/1")
            out += SwiftUI._children(c, "\(path)/2")
            out += SwiftUI._children(d, "\(path)/3")
            out += SwiftUI._children(e, "\(path)/4")
            out += SwiftUI._children(f, "\(path)/5")
            out += SwiftUI._children(g, "\(path)/6")
            return out
        }
    }
    public static func buildBlock<A: View, B: View, C: View, D: View, E: View, F: View, G: View, H: View>(_ a: A, _ b: B, _ c: C, _ d: D, _ e: E, _ f: F, _ g: G, _ h: H) -> TupleView<(A, B, C, D, E, F, G, H)> {
        .init((a, b, c, d, e, f, g, h)) { path in
            var out: [Node] = []
            out.reserveCapacity(8)
            out += SwiftUI._children(a, "\(path)/0")
            out += SwiftUI._children(b, "\(path)/1")
            out += SwiftUI._children(c, "\(path)/2")
            out += SwiftUI._children(d, "\(path)/3")
            out += SwiftUI._children(e, "\(path)/4")
            out += SwiftUI._children(f, "\(path)/5")
            out += SwiftUI._children(g, "\(path)/6")
            out += SwiftUI._children(h, "\(path)/7")
            return out
        }
    }
    public static func buildBlock<A: View, B: View, C: View, D: View, E: View, F: View, G: View, H: View, I: View>(_ a: A, _ b: B, _ c: C, _ d: D, _ e: E, _ f: F, _ g: G, _ h: H, _ i: I) -> TupleView<(A, B, C, D, E, F, G, H, I)> {
        .init((a, b, c, d, e, f, g, h, i)) { path in
            var out: [Node] = []
            out.reserveCapacity(9)
            out += SwiftUI._children(a, "\(path)/0")
            out += SwiftUI._children(b, "\(path)/1")
            out += SwiftUI._children(c, "\(path)/2")
            out += SwiftUI._children(d, "\(path)/3")
            out += SwiftUI._children(e, "\(path)/4")
            out += SwiftUI._children(f, "\(path)/5")
            out += SwiftUI._children(g, "\(path)/6")
            out += SwiftUI._children(h, "\(path)/7")
            out += SwiftUI._children(i, "\(path)/8")
            return out
        }
    }
    public static func buildBlock<A: View, B: View, C: View, D: View, E: View, F: View, G: View, H: View, I: View, J: View>(_ a: A, _ b: B, _ c: C, _ d: D, _ e: E, _ f: F, _ g: G, _ h: H, _ i: I, _ j: J) -> TupleView<(A, B, C, D, E, F, G, H, I, J)> {
        .init((a, b, c, d, e, f, g, h, i, j)) { path in
            var out: [Node] = []
            out.reserveCapacity(10)
            out += SwiftUI._children(a, "\(path)/0")
            out += SwiftUI._children(b, "\(path)/1")
            out += SwiftUI._children(c, "\(path)/2")
            out += SwiftUI._children(d, "\(path)/3")
            out += SwiftUI._children(e, "\(path)/4")
            out += SwiftUI._children(f, "\(path)/5")
            out += SwiftUI._children(g, "\(path)/6")
            out += SwiftUI._children(h, "\(path)/7")
            out += SwiftUI._children(i, "\(path)/8")
            out += SwiftUI._children(j, "\(path)/9")
            return out
        }
    }
}

public struct TupleView<T>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let value: T
    let emit: ((String) -> [Node])?
    init(_ v: T) { value = v; emit = nil }
    init(_ v: T, _ emit: @escaping (String) -> [Node]) { value = v; self.emit = emit }
    func _children(_ path: String) -> [Node] {
        if let emit { return emit(path) }
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

public struct _ConditionalContent<T, F> {
    enum Storage { case first(T), second(F) }
    let storage: Storage
    init(_ s: Storage) { storage = s }
}

extension _ConditionalContent: View, _Flattenable where T: View, F: View {
    public typealias Body = Never
    public var body: Never { fatalError() }
    func _children(_ path: String) -> [Node] {
        switch storage {
        case .first(let v): return SwiftUI._children(v, path + "/T")
        case .second(let v): return SwiftUI._children(v, path + "/F")
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
    var sliderEdits: [String: (Bool) -> Void] = [:]
    var textInput = _TextInputContext()
    var textFocus: [String: [_TextFocus]] = [:]
    var disabled = false
    var buttonStyle = "automatic"
    var pickerStyle = "menu"
    var styles = _Styles()   // control styles and environment values (section Q)
    var pageTabStyle: PageTabViewStyle?
    var pageIndexStyle = PageIndexViewStyle()
    var pageIsActive = true   // adjacent pages are laid out for swiping but do not appear or present yet
    var tags: [String: AnyHashable] = [:]   // `.tag(_:)` values by node id, read by `Picker`
    var rowMeta: [String: _RowMeta] = [:]   // list row delete/move/swipe actions by node id, read by `List` (section T)
    var pasteboard: [String] = []   // the session pasteboard (SIMCORE_PASTEBOARD at launch, then `pasteboard` lines)
    var rowContextMenu: _RowContextMenu?   // `contextMenu(forSelectionType:)` for the ForEach rows below
    var rowActions = _RowActions()          // `onDelete` / `onMove` for the ForEach being evaluated
    var nav: _NavState?
    // Container modifiers finish after their navigation node has been emitted.
    var emittedNavigationStates: [String: _NavState] = [:]
    var navigationPreferences: Node?
    var envObjects: [ObjectIdentifier: Any] = [:]   // `.environmentObject` / `.environment` objects and EnvironmentKey values
    var envLocal = Set<ObjectIdentifier>()   // envObjects keys set inside the enclosing NavigationStack (pushed screens re-apply only these)
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
    var visualEffectFrames: [String: CGRect] = [:]
    var visualEffectIDs: Set<String> = []
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
    // Animation & transitions (section P).
    var animToken: UInt64 = 0
    var currentTransaction: Transaction? = nil
    var pendingTransaction: Transaction? = nil
    var activeTransaction: Transaction? = nil
    var animSeen: [String: Any] = [:], animNow: [String: Any] = [:]
    var contentTransition: ContentTransition = .identity
    var contentTransitionAddsDrawingGroup = false
    var clockMs: Double = 0
    var baseEpoch: Double = 1_700_000_000
    var nextScheduleMs: Double? = nil
    var lastScheduledMs: Double? = nil
    var tickHandlers: [() -> Void] = []
    var completions: [(dueMs: Double, fn: () -> Void)] = []
    /// The app's scene phase (host `scene_phase`). `App.main` launches inactive; a bare root view starts active.
    var scenePhase = ScenePhase.active
    /// `.defaultAppStorage` in scope (nil = `UserDefaults.standard`).
    var appStorage: UserDefaults?
    #if !os(WASI)
    /// `AsyncImage` HTTP loads. Nil uses `URLSession.shared`. Set by `.asyncImageURLSession`.
    var imageSession: URLSession?
    /// `onOpenURL` handlers of this evaluation, in view order.
    var urlHandlers: [(URL) -> Void] = []
    #endif

    var currentDate: Date { Date(timeIntervalSince1970: baseEpoch + clockMs / 1000.0) }

    func nextAnimToken() -> UInt64 {
        animToken += 1
        return animToken
    }

    private func _mergeTransaction(_ prev: Transaction?, _ tx: Transaction) -> Transaction {
        guard var merged = prev else { return tx }
        if tx.disablesAnimations { merged.disablesAnimations = true }
        if let a = tx.animation { merged.animation = a }
        for (k, v) in tx.custom { merged.custom[k] = v }
        merged.completions.append(contentsOf: tx.completions)
        return merged
    }

    func markDirty() {
        dirty = true
        if var tx = currentTransaction {
            pendingTransaction = _mergeTransaction(pendingTransaction, tx)
            if !tx.completions.isEmpty {
                tx.completions.removeAll()
                currentTransaction = tx
            }
        }
    }

    func requestSchedule(_ afterMs: Double) {
        let clamped = max(0.0, afterMs)
        if let cur = nextScheduleMs {
            if clamped < cur { nextScheduleMs = clamped }
        } else {
            nextScheduleMs = clamped
        }
    }

    func noteAnimValue<V: Equatable>(_ path: String, _ value: V) -> Bool {
        var k = path
        while animNow[k] != nil { k += "'" }
        animNow[k] = value
        if let old = animSeen[k] as? V { return old != value }
        return false
    }

    /// Compare `value` with the previous pass's; queue `fire(old, new)` on a change (or on first sight when `initial`).
    func noteChange<V: Equatable>(_ path: String, _ value: V, initial: Bool, _ fire: @escaping (V, V) -> Void) {
        var k = path
        while changeNow[k] != nil { k += "'" }   // several onChange on one view
        changeNow[k] = value
        if let old = changeSeen[k] as? V { if old != value { changePending.append { fire(old, value) } } }
        else if initial { changePending.append { fire(value, value) } }
    }

    func noteLifecycle(_ path: String, _ appear: (() -> Void)?, _ disappear: (() -> Void)?) {
        guard pageIsActive else { return }
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

    /// Launch: the scene's first body runs while the app is still inactive (state, `onChange(initial:)`), so the
    /// change to active is seen by `onChange(of: scenePhase)` in the first submitted pass, as on iOS.
    func primeEvaluation() {
        _pool {
            _ = _evaluate()
            let fire = changePending; changePending = []
            fire.forEach { $0() }
        }
    }

    /// Set by `_onesimStart`; the first submission (native: at once; web: when the page pushes the environment)
    /// connects the scene: one pass while inactive for an `App`, then active.
    var launching = false

    /// Re-evaluate the whole root, drop stale actions/state, and submit.
    func submitTree(emitSchedule: Bool = true) {
        if launching {
            launching = false
            if scenePhase != .active { primeEvaluation() }   // an `App`: its scene connects inactive
            _sceneDidBecomeActive()
        }
        _pool { _submitTree(emitSchedule: emitSchedule) }
    }

    private func _evaluate() -> Node {
        actions.removeAll(keepingCapacity: true)
        fields.removeAll(keepingCapacity: true)
        let previousFocus = textFocus
        textFocus.removeAll(keepingCapacity: true)
        textInput = _TextInputContext()
        disabled = false; buttonStyle = "automatic"; pickerStyle = "menu"; styles = _Styles(); tags = [:]; rowMeta = [:]; rowActions = _RowActions(); nav = nil; envObjects = [:]; envLocal = []
        pageTabStyle = nil; pageIndexStyle = .init(); pageIsActive = true
        #if !os(WASI)
        imageSession = nil
        urlHandlers.removeAll(keepingCapacity: true)
        #endif
        appStorage = nil
        layers = []; autoDismiss = nil; dismissHook = nil
        lifecycleNow = [:]; lifecycleOrder = []; changeNow = [:]; changePending = []
        animNow = [:]; contentTransition = .identity; contentTransitionAddsDrawingGroup = false
        nextScheduleMs = nil; tickHandlers = []
        visualEffectIDs.removeAll(keepingCapacity: true)
        gestures = [:]; gesturesOff = false
        scrollStack.removeAll(keepingCapacity: true)
        anchorScrollNow.removeAll(keepingCapacity: true)
        scrollPos.removeAll(keepingCapacity: true)
        touched.removeAll(keepingCapacity: true)
        // Any `@Observable` property read while evaluating dirties the tree when it next changes (one-shot; re-armed each pass).
        var tree = withObservationTracking(render) { _Runtime.shared.dirty = true }
        emittedNavigationStates.removeAll(keepingCapacity: true)
        if !layers.isEmpty { tree = ["kind": "presentation", "id": "presentation", "content": tree, "layers": layers] }
        slots = slots.filter { touched.contains($0.key) }
        visualEffectFrames = visualEffectFrames.filter { visualEffectIDs.contains($0.key) }
        changeSeen = changeNow
        animSeen = animNow
        anchorScroll = anchorScrollNow   // a later button action (stack empty) can still scrollTo
        dirty = false
        for target in previousFocus.values.joined() where target.isFocused() {
            if !textFocus.values.joined().contains(where: { $0.matches(target) }) { target.blur() }
        }
        return tree
    }

    private func _submitTree(emitSchedule: Bool = true) {
        var tx = pendingTransaction
        pendingTransaction = nil
        activeTransaction = tx
        var tree = _evaluate()
        // Lifecycle callbacks run after the pass; if they dirtied state (including via withAnimation /
        // withTransaction), consume their transaction into this submission and re-evaluate (capped).
        for _ in 0..<8 {
            _lifecycle()
            let fire = changePending; changePending = []
            fire.forEach { $0() }
            if let nextTx = pendingTransaction {
                pendingTransaction = nil
                tx = _mergeTransaction(tx, nextTx)
                activeTransaction = tx
            }
            guard dirty else { break }
            tree = _evaluate()
        }
        activeTransaction = nil
        // Attach any explicit `withAnimation` / `withTransaction` animation to the root node.
        if let tx {
            if tx.disablesAnimations {
                var mods = (tree["modifiers"] as? [Node]) ?? []
                mods.append(["animation": NSNull()])
                tree["modifiers"] = mods
                for fn in tx.completions { fn() }
            } else if let anim = tx.animation {
                let tok = nextAnimToken()
                var mods = (tree["modifiers"] as? [Node]) ?? []
                mods.append(["animation": anim._wire(token: tok)])
                tree["modifiers"] = mods
                if !tx.completions.isEmpty {
                    let durMs = anim._totalDuration * 1000.0
                    for fn in tx.completions { completions.append((dueMs: clockMs + durMs, fn: fn)) }
                }
            } else if !tx.completions.isEmpty {
                for fn in tx.completions { fn() }
            }
        }
        // Include pending animation completion timers in the requested schedule.
        for c in completions {
            requestSchedule(max(0.0, c.dueMs - clockMs))
        }
        // Emit schedule before tree on normal renders so host `send_and_wait_tree` sees both together.
        if emitSchedule { _sendSchedule(force: false) }
        dirty = false
        version += 1
        #if os(WASI)
        var json = ""
        json.reserveCapacity(4096)
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

    func _sendSchedule(force: Bool) {
        if force || nextScheduleMs != lastScheduledMs || nextScheduleMs != nil {
            lastScheduledMs = nextScheduleMs
            var m: Node = ["type": "schedule"]
            if let ms = nextScheduleMs { m["afterMs"] = ms }
            _send(m)
        }
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

/// Entry point of generated mains (a root view without an `App`).
public func _onesimRun<V: View>(_ root: V) -> _Done {
    _onesimStart { _withEditMode("0") { _eval(root, "0") } }
}

#if os(WASI)
/// Entry point on WASI (reactor): registers the app and returns. The first render happens when
/// onesim-web pushes the environment (simcore_web_init); the browser is the event loop.
func _onesimStart(_ render: @escaping () -> Node) {
    let rt = _Runtime.shared
    rt.render = render
    _onesimRegisterCallbacks()
    rt.launching = true
}
#else
/// Starts the runtime with `render` as the root and runs the event loop. Never returns.
func _onesimStart(_ render: @escaping () -> Node) -> Never {
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
    if let pb = env["SIMCORE_PASTEBOARD"] { rt.pasteboard = [pb] }
    if let t = env["SIMCORE_TIME"].flatMap(Double.init) { rt.baseEpoch = t } else { rt.baseEpoch = Date().timeIntervalSince1970 }
    rt.render = render
    _onesimRegisterCallbacks()
    _installImageWake()
    rt.launching = true
    rt.submitTree()
    let rc = simcore_runtime_run()
    _appWillTerminate()
    exit(rc)
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
            if rt.visualEffectIDs.contains(id) {
                let frame = CGRect(x: _f64(box[0]), y: _f64(box[1]), width: size.width, height: size.height)
                if rt.visualEffectFrames[id] != frame { rt.visualEffectFrames[id] = frame; rt.dirty = true }
            } else if rt.shapeSizes[id] != size {
                rt.shapeSizes[id] = size
                rt.dirty = true
            }
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
    } else if type == "pasteboard" {
        let items = (obj["items"] as? [Any])?.compactMap { $0 as? String } ?? []
        if rt.pasteboard != items { rt.pasteboard = items; rt.dirty = true }
    } else if type == "scene_phase" {
        _applyScenePhase(obj["phase"] as? String ?? "")
    } else if type == "open_url" {
        #if os(WASI)
        rt.submitTree()   // no Foundation `URL` on the web build: nothing receives it
        #else
        _applyOpenURL(obj["url"] as? String ?? "")
        #endif
    } else if type == "scroll_position", let scroll = obj["scroll"] as? String {
        rt.scrollPos[scroll]?(obj["target"] as? String)   // NSNull target becomes nil
    } else if type == "tick" {
        rt.clockMs = _f64(obj["time"])
        rt.nextScheduleMs = nil
        // Fire due animation completion callbacks.
        var remaining: [(dueMs: Double, fn: () -> Void)] = []
        var dueFns: [() -> Void] = []
        for c in rt.completions {
            if c.dueMs <= rt.clockMs + 0.5 { dueFns.append(c.fn) } else { remaining.append(c) }
        }
        rt.completions = remaining
        for fn in dueFns { fn() }
        // Advance active TimelineView / PhaseAnimator / KeyframeAnimator handlers.
        let handlers = rt.tickHandlers
        for h in handlers { h() }
        // Emit any updated tree first, then terminate the tick response with `schedule`
        // so the host waiting on `schedule` observes both atomically.
        if rt.dirty {
            rt.submitTree(emitSchedule: false)
        } else {
            for c in rt.completions { rt.requestSchedule(max(0.0, c.dueMs - rt.clockMs)) }
        }
        rt._sendSchedule(force: true)
    }
}
