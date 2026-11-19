#if os(WASI)
import WASILibc
#else
import Foundation
#endif
#if canImport(CoreGraphics)
// Apple's SwiftUI re-exports CoreGraphics (CGFloat, CGPoint, CGSize in gesture values).
@_exported import CoreGraphics
#endif

// Gestures (docs/protocol.md section L). A view with gestures carries a `gesture` modifier naming them by a key
// (its structural path); the host sends every touch on such views, with the keys under the finger innermost first.
// Recognition, composition and arbitration happen here: each touch sequence builds recognizers from the gestures
// registered by the latest evaluation, feeds them every event, and runs the winners' callbacks.

// MARK: - Geometry

public struct Angle: Equatable, Hashable, Comparable {
    public var radians: Double
    public var degrees: Double {
        get { radians * 180 / .pi }
        set { radians = newValue * .pi / 180 }
    }
    public init() { radians = 0 }
    public init(radians: Double) { self.radians = radians }
    public init(degrees: Double) { radians = degrees * .pi / 180 }
    public static func radians(_ r: Double) -> Angle { Angle(radians: r) }
    public static func degrees(_ d: Double) -> Angle { Angle(degrees: d) }
    public static let zero = Angle()
    public static func < (a: Angle, b: Angle) -> Bool { a.radians < b.radians }
    public static func + (a: Angle, b: Angle) -> Angle { Angle(radians: a.radians + b.radians) }
    public static func - (a: Angle, b: Angle) -> Angle { Angle(radians: a.radians - b.radians) }
    public static prefix func - (a: Angle) -> Angle { Angle(radians: -a.radians) }
    public static func * (a: Angle, k: Double) -> Angle { Angle(radians: a.radians * k) }
    public static func / (a: Angle, k: Double) -> Angle { Angle(radians: a.radians / k) }
    public static func += (a: inout Angle, b: Angle) { a = a + b }
    public static func -= (a: inout Angle, b: Angle) { a = a - b }
}

public struct UnitPoint: Equatable, Hashable {
    public var x: CGFloat, y: CGFloat
    public init() { x = 0; y = 0 }
    public init(x: CGFloat, y: CGFloat) { self.x = x; self.y = y }
    public static let zero = UnitPoint(x: 0, y: 0)
    public static let center = UnitPoint(x: 0.5, y: 0.5)
    public static let leading = UnitPoint(x: 0, y: 0.5)
    public static let trailing = UnitPoint(x: 1, y: 0.5)
    public static let top = UnitPoint(x: 0.5, y: 0)
    public static let bottom = UnitPoint(x: 0.5, y: 1)
    public static let topLeading = UnitPoint(x: 0, y: 0)
    public static let topTrailing = UnitPoint(x: 1, y: 0)
    public static let bottomLeading = UnitPoint(x: 0, y: 1)
    public static let bottomTrailing = UnitPoint(x: 1, y: 1)
}

/// Gesture locations: `.local` (the view's own frame) or `.global` (the screen). A named space is the screen too.
public enum CoordinateSpace: Equatable, Hashable {
    case global, local
    case named(AnyHashable)
}

/// Which gestures a `.gesture` modifier enables: its own (`.gesture`), the subviews' (`.subviews`), both or none.
public struct GestureMask: OptionSet {
    public let rawValue: UInt32
    public init(rawValue: UInt32) { self.rawValue = rawValue }
    public static let none = GestureMask([])
    public static let gesture = GestureMask(rawValue: 1)
    public static let subviews = GestureMask(rawValue: 2)
    public static let all = GestureMask(rawValue: 3)
}

/// Passed to `updating` bodies. Animations are not simulated, so it carries no animation.
public struct Transaction {
    public var disablesAnimations = false
    public var isContinuous = false
    public init() {}
}

// MARK: - Gesture

public protocol Gesture<Value> {
    associatedtype Value
    associatedtype Body: Gesture
    var body: Body { get }
    /// A fresh recognizer for one touch sequence. `id` names it across sequences (tap counts).
    func _recognizer(_ id: String) -> _GestureRecognizer<Value>
}

extension Gesture where Body == Never {
    public var body: Never { fatalError("\(Self.self) is a primitive gesture") }
}

/// Custom gestures recognize their `body`.
extension Gesture where Value == Body.Value {
    public func _recognizer(_ id: String) -> _GestureRecognizer<Value> { body._recognizer(id) }
}

extension Never: Gesture {
    public typealias Value = Never
    public func _recognizer(_ id: String) -> _GestureRecognizer<Never> { fatalError() }
}

// MARK: - Recognizers

/// One touch event as one gesture view sees it: screen points, and that view's frame for local coordinates.
struct _Touch {
    enum Phase: Int32 { case start, move, end, cancel }
    var phase: Phase
    var time: Double   // ms
    var points: [CGPoint]
    var origin: CGPoint
    var size: CGSize

    func local(_ p: CGPoint, _ space: CoordinateSpace) -> CGPoint {
        space == .local ? CGPoint(x: p.x - origin.x, y: p.y - origin.y) : p
    }
}

/// Callbacks a recognizer produced for one event. `actions` run only when its gesture may act (it won, or nothing has
/// won and it leads); `resets` (`@GestureState` back to its initial value, pressing off) always run.
final class _Queue {
    var actions: [() -> Void] = []
    var resets: [() -> Void] = []
}

enum _Status { case possible, tentative, active, ended, failed }

protocol _Recognizing: AnyObject {
    var status: _Status { get }
    var pans: Bool { get }
    func feed(_ t: _Touch, _ q: _Queue)
    func cancel(_ q: _Queue)
}

/// Recognizes one gesture over one touch sequence. `tentative` = producing values but not yet committed (a long
/// press before its duration), `active` = committed and changing, `ended` = succeeded.
public class _GestureRecognizer<Value>: _Recognizing {
    var status = _Status.possible
    var value: Value?
    /// The last event changed `value` while the gesture is under way.
    var changed = false
    /// The last event ended / failed the gesture.
    var endedNow = false, failedNow = false
    /// Follows a moving finger (a scroller under it must not take the touch).
    var pans: Bool { false }

    init() {}

    final func feed(_ t: _Touch, _ q: _Queue) {
        changed = false; endedNow = false; failedNow = false
        guard status != .ended && status != .failed else { return }
        _feed(t, q)
        endedNow = status == .ended
        failedNow = status == .failed
    }

    /// Lost to another gesture, or the touch was taken away.
    final func cancel(_ q: _Queue) {
        guard status != .ended && status != .failed else { return }
        _cancel(q)
        status = .failed
        failedNow = true
    }

    func _feed(_ t: _Touch, _ q: _Queue) {}
    func _cancel(_ q: _Queue) {}

    func fail() { status = .failed }
}

func _distance(_ a: CGPoint, _ b: CGPoint) -> CGFloat { ((a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y)).squareRoot() }

/// Time (ms) to a gesture value `Date`.
func _date(_ ms: Double) -> Date { Date(timeIntervalSince1970: ms / 1000) }

/// A finger may move this far (pt) and still tap.
let _tapSlop: CGFloat = 10
/// Taps closer together than this (ms) count towards one multi-tap.
let _multiTapInterval: Double = 350

/// Per-recognizer memory across touch sequences: taps so far towards a multi-tap.
var _tapMemory: [String: (count: Int, time: Double, at: CGPoint)] = [:]

final class _TapR<V>: _GestureRecognizer<V> {
    let id: String, count: Int, space: CoordinateSpace, make: (CGPoint) -> V
    var start = CGPoint()
    init(_ id: String, count: Int, space: CoordinateSpace, make: @escaping (CGPoint) -> V) {
        self.id = id; self.count = max(1, count); self.space = space; self.make = make
    }
    override func _feed(_ t: _Touch, _ q: _Queue) {
        guard let p = t.points.first else { return fail() }
        switch t.phase {
        case .start:
            start = p
            if t.points.count > 1 { fail() }
        case .move:
            if t.points.count > 1 || _distance(p, start) >= _tapSlop { fail() }
        case .end:
            if _distance(p, start) >= _tapSlop { return fail() }
            let prev = _tapMemory[id]
            let n = prev.map { t.time - $0.time <= _multiTapInterval && _distance($0.at, p) < 45 ? $0.count + 1 : 1 } ?? 1
            if n >= count {
                _tapMemory[id] = nil
                value = make(t.local(p, space))
                status = .ended
            } else {
                _tapMemory[id] = (n, t.time, p)
                fail()
            }
        case .cancel: fail()
        }
    }
}

final class _LongPressR: _GestureRecognizer<Bool> {
    let duration: Double, distance: CGFloat
    var start = CGPoint(), t0 = 0.0
    init(_ duration: Double, _ distance: CGFloat) { self.duration = duration; self.distance = distance }
    override func _feed(_ t: _Touch, _ q: _Queue) {
        guard let p = t.points.first else { return fail() }
        switch t.phase {
        case .start:
            start = p; t0 = t.time
            value = true; changed = true
            status = duration <= 0 ? .ended : .tentative
        case .move:
            if _distance(p, start) > distance { return fail() }
            if t.time - t0 >= duration * 1000 { status = .ended }
        case .end:
            if _distance(p, start) <= distance && t.time - t0 >= duration * 1000 { status = .ended } else { fail() }
        case .cancel: fail()
        }
    }
}

final class _DragR: _GestureRecognizer<DragGesture.Value> {
    let minimum: CGFloat, space: CoordinateSpace
    var start = CGPoint(), samples: [(Double, CGPoint)] = []
    override var pans: Bool { true }
    init(_ minimum: CGFloat, _ space: CoordinateSpace) { self.minimum = minimum; self.space = space }

    override func _feed(_ t: _Touch, _ q: _Queue) {
        guard let g = t.points.first else { return fail() }
        let p = t.local(g, space)
        switch t.phase {
        case .start:
            start = p
            samples = [(t.time, p)]
            if minimum <= 0 { status = .active; update(t.time, p) }
        case .move:
            samples.append((t.time, p))
            samples.removeAll { t.time - $0.0 > 100 }
            if status == .possible && _distance(p, start) >= minimum { status = .active }
            if status == .active { update(t.time, p) }
        case .end:
            guard status == .active else { return fail() }
            samples.append((t.time, p))
            samples.removeAll { t.time - $0.0 > 100 }
            update(t.time, p)
            changed = false
            status = .ended
        case .cancel: fail()
        }
    }

    private func update(_ time: Double, _ p: CGPoint) {
        var v = CGSize()
        if let a = samples.first, time > a.0 {
            let dt = CGFloat((time - a.0) / 1000)
            v = CGSize(width: (p.x - a.1.x) / dt, height: (p.y - a.1.y) / dt)
        }
        // ponytail: where a flick would carry on to, as 0.25 s of the release velocity (not UIKit's deceleration curve).
        let pe = CGPoint(x: p.x + v.width / 4, y: p.y + v.height / 4)
        value = DragGesture.Value(time: _date(time), location: p, startLocation: start, velocity: v, predictedEndLocation: pe)
        changed = true
    }
}

/// Two fingers: scale and rotation relative to where they first were both down.
struct _TwoFingers {
    var time = 0.0, scale: CGFloat = 1, angle = 0.0, scaleVelocity: CGFloat = 0, angleVelocity = 0.0
    var startLocation = CGPoint(), startAnchor = UnitPoint.center
}

final class _TwoFingerR: _GestureRecognizer<_TwoFingers> {
    /// Activates once the scale changes by this much (`rotation` false) or the angle by this many radians.
    let threshold: Double, rotation: Bool
    var d0: CGFloat = 0, a0 = 0.0, last: (Double, CGFloat, Double)?
    override var pans: Bool { true }
    init(threshold: Double, rotation: Bool) { self.threshold = threshold; self.rotation = rotation }

    override func _feed(_ t: _Touch, _ q: _Queue) {
        if t.phase == .cancel { return fail() }
        guard t.points.count >= 2 else {
            if t.phase == .end { if status == .active { finish() } else { fail() } }
            return
        }
        let (a, b) = (t.points[0], t.points[1])
        let d = _distance(a, b), ang = atan2(Double(b.y - a.y), Double(b.x - a.x))
        if d0 == 0 {
            d0 = max(d, 1); a0 = ang
            let c = CGPoint(x: (a.x + b.x) / 2, y: (a.y + b.y) / 2)
            var v = _TwoFingers(time: t.time)
            v.startLocation = t.local(c, .local)
            v.startAnchor = UnitPoint(x: t.size.width > 0 ? (c.x - t.origin.x) / t.size.width : 0.5,
                                      y: t.size.height > 0 ? (c.y - t.origin.y) / t.size.height : 0.5)
            value = v
            last = (t.time, 1, 0)
            if t.phase != .end { return }
        }
        var v = value ?? _TwoFingers()
        var da = ang - a0
        while da > .pi { da -= 2 * .pi }
        while da < -.pi { da += 2 * .pi }
        let s = d / d0
        if let (lt, ls, la) = last, t.time > lt {
            let dt = (t.time - lt) / 1000
            v.scaleVelocity = (s - ls) / CGFloat(dt); v.angleVelocity = (da - la) / dt
        }
        last = (t.time, s, da)
        v.time = t.time; v.scale = s; v.angle = da
        value = v
        if status == .possible && (rotation ? abs(da) >= threshold : abs(Double(s) - 1) >= threshold) { status = .active }
        if t.phase == .end {
            if status == .active { finish() } else { fail() }
        } else if status == .active {
            changed = true
        }
    }

    private func finish() { status = .ended }
}

/// Mirrors an inner recognizer, with its value mapped.
class _WrapR<V, W>: _GestureRecognizer<W> {
    let inner: _GestureRecognizer<V>
    let f: (V) -> W
    override var pans: Bool { inner.pans }
    init(_ inner: _GestureRecognizer<V>, _ f: @escaping (V) -> W) { self.inner = inner; self.f = f }
    override func _feed(_ t: _Touch, _ q: _Queue) {
        inner.feed(t, q)
        sync()
    }
    override func _cancel(_ q: _Queue) { inner.cancel(q) }
    func sync() {
        status = inner.status
        value = inner.value.map(f)
        changed = inner.changed
    }
}

final class _ChangedR<V>: _WrapR<V, V> {
    let action: (V) -> Void
    init(_ inner: _GestureRecognizer<V>, _ action: @escaping (V) -> Void) { self.action = action; super.init(inner, { $0 }) }
    override func _feed(_ t: _Touch, _ q: _Queue) {
        super._feed(t, q)
        if changed, let v = value, status == .tentative || status == .active { let a = action; q.actions.append { a(v) } }
    }
}

final class _EndedR<V>: _WrapR<V, V> {
    let action: (V) -> Void
    init(_ inner: _GestureRecognizer<V>, _ action: @escaping (V) -> Void) { self.action = action; super.init(inner, { $0 }) }
    override func _feed(_ t: _Touch, _ q: _Queue) {
        super._feed(t, q)
        if status == .ended, let v = value { let a = action; q.actions.append { a(v) } }
    }
}

final class _UpdatingR<V, S>: _WrapR<V, V> {
    let state: GestureState<S>, body: (V, inout S, inout Transaction) -> Void
    init(_ inner: _GestureRecognizer<V>, _ state: GestureState<S>, _ body: @escaping (V, inout S, inout Transaction) -> Void) {
        self.state = state; self.body = body; super.init(inner, { $0 })
    }
    override func _feed(_ t: _Touch, _ q: _Queue) {
        super._feed(t, q)
        let (st, b) = (state, body)
        if changed, let v = value, status == .tentative || status == .active {
            q.actions.append { var s = st.wrappedValue; var tr = Transaction(); b(v, &s, &tr); st._set(s) }
        }
        if status == .ended || status == .failed { q.resets.append { st._reset() } }
    }
    override func _cancel(_ q: _Queue) {
        super._cancel(q)
        let st = state
        q.resets.append { st._reset() }
    }
}

/// `onLongPressGesture(onPressingChanged:)`: pressing while the press is under way, off when it ends either way.
final class _PressingR: _WrapR<Bool, Bool> {
    let pressing: (Bool) -> Void
    var on = false
    init(_ inner: _GestureRecognizer<Bool>, _ pressing: @escaping (Bool) -> Void) { self.pressing = pressing; super.init(inner, { $0 }) }
    override func _feed(_ t: _Touch, _ q: _Queue) {
        super._feed(t, q)
        let p = pressing
        // A zero-duration press succeeds at touch down: it still presses first.
        if !on && (status == .tentative || (status == .ended && t.phase == .start)) { on = true; q.actions.append { p(true) } }
        if (status == .ended || status == .failed) && on { on = false; q.resets.append { p(false) } }
    }
    override func _cancel(_ q: _Queue) {
        super._cancel(q)
        let p = pressing
        if on { on = false; q.resets.append { p(false) } }
    }
}

final class _SimultaneousR<A: Gesture, B: Gesture>: _GestureRecognizer<SimultaneousGesture<A, B>.Value> {
    let a: _GestureRecognizer<A.Value>, b: _GestureRecognizer<B.Value>
    override var pans: Bool { a.pans || b.pans }
    init(_ a: _GestureRecognizer<A.Value>, _ b: _GestureRecognizer<B.Value>) { self.a = a; self.b = b }
    override func _feed(_ t: _Touch, _ q: _Queue) {
        a.feed(t, q); b.feed(t, q)
        let s = [a.status, b.status]
        if s.allSatisfy({ $0 == .failed }) { status = .failed }
        else if s.allSatisfy({ $0 == .ended || $0 == .failed }) { status = .ended }
        else if s.contains(.active) || s.contains(.ended) { status = .active }
        else if s.contains(.tentative) { status = .tentative }
        value = .init(first: a.status == .failed ? nil : a.value, second: b.status == .failed ? nil : b.value)
        changed = a.changed || b.changed
    }
    override func _cancel(_ q: _Queue) { a.cancel(q); b.cancel(q) }
}

final class _SequenceR<A: Gesture, B: Gesture>: _GestureRecognizer<SequenceGesture<A, B>.Value> {
    let a: _GestureRecognizer<A.Value>, b: _GestureRecognizer<B.Value>
    var second = false
    override var pans: Bool { a.pans || b.pans }
    init(_ a: _GestureRecognizer<A.Value>, _ b: _GestureRecognizer<B.Value>) { self.a = a; self.b = b }
    override func _feed(_ t: _Touch, _ q: _Queue) {
        if !second {
            a.feed(t, q)
            switch a.status {
            case .failed: return fail()
            case .ended:
                // The second gesture starts where the finger is when the first one completes.
                second = true
                let av = a.value!
                value = .second(av, nil); changed = true
                if t.phase == .end || t.phase == .cancel { return fail() }
                var s = t; s.phase = .start
                b.feed(s, q)
                if b.status == .failed { return fail() }
                status = .active
                if b.status == .active, let bv = b.value { value = .second(av, bv) }
            default:
                status = a.status
                if let av = a.value, a.changed { value = .first(av); changed = true }
            }
            return
        }
        b.feed(t, q)
        guard let av = a.value else { return fail() }
        switch b.status {
        case .failed: fail()
        case .ended: value = .second(av, b.value); status = .ended
        default: if b.changed { value = .second(av, b.value); changed = true }
        }
    }
    override func _cancel(_ q: _Queue) { a.cancel(q); b.cancel(q) }
}

final class _ExclusiveR<A: Gesture, B: Gesture>: _GestureRecognizer<ExclusiveGesture<A, B>.Value> {
    let a: _GestureRecognizer<A.Value>, b: _GestureRecognizer<B.Value>
    var chosen: Int?
    override var pans: Bool { a.pans || b.pans }
    init(_ a: _GestureRecognizer<A.Value>, _ b: _GestureRecognizer<B.Value>) { self.a = a; self.b = b }
    override func _feed(_ t: _Touch, _ q: _Queue) {
        if chosen != 0 { b.feed(t, q) }
        if chosen != 1 { a.feed(t, q) }
        if chosen == nil {
            // The first has priority: it wins once it commits; the second wins when it commits first, or the first fails.
            if a.status == .active || a.status == .ended { chosen = 0; b.cancel(q) }
            else if b.status == .active || b.status == .ended || a.status == .failed { chosen = 1; a.cancel(q) }
        }
        switch chosen {
        case 0: status = a.status; if let v = a.value { value = .first(v) }; changed = a.changed
        case 1: status = b.status; if let v = b.value { value = .second(v) }; changed = b.changed
        default: status = a.status == .tentative || b.status == .tentative ? .tentative : .possible
        }
    }
    override func _cancel(_ q: _Queue) { a.cancel(q); b.cancel(q) }
}

// MARK: - Primitive gestures

public struct TapGesture: Gesture {
    public typealias Value = Void
    public typealias Body = Never
    public var count: Int
    public init(count: Int = 1) { self.count = count }
    public func _recognizer(_ id: String) -> _GestureRecognizer<Void> { _TapR(id, count: count, space: .local) { _ in () } }
}

public struct SpatialTapGesture: Gesture {
    public struct Value: Equatable { public var location: CGPoint }
    public typealias Body = Never
    public var count: Int
    public var coordinateSpace: CoordinateSpace
    public init(count: Int = 1, coordinateSpace: CoordinateSpace = .local) { self.count = count; self.coordinateSpace = coordinateSpace }
    public func _recognizer(_ id: String) -> _GestureRecognizer<Value> { _TapR(id, count: count, space: coordinateSpace) { Value(location: $0) } }
}

public struct LongPressGesture: Gesture {
    public typealias Value = Bool
    public typealias Body = Never
    public var minimumDuration: Double
    public var maximumDistance: CGFloat
    public init(minimumDuration: Double = 0.5, maximumDistance: CGFloat = 10) { self.minimumDuration = minimumDuration; self.maximumDistance = maximumDistance }
    public func _recognizer(_ id: String) -> _GestureRecognizer<Bool> { _LongPressR(minimumDuration, maximumDistance) }
}

public struct DragGesture: Gesture {
    public struct Value: Equatable {
        public var time: Date
        public var location: CGPoint
        public var startLocation: CGPoint
        public var velocity: CGSize
        public var predictedEndLocation: CGPoint
        public var translation: CGSize { CGSize(width: location.x - startLocation.x, height: location.y - startLocation.y) }
        public var predictedEndTranslation: CGSize {
            CGSize(width: predictedEndLocation.x - startLocation.x, height: predictedEndLocation.y - startLocation.y)
        }
    }
    public typealias Body = Never
    public var minimumDistance: CGFloat
    public var coordinateSpace: CoordinateSpace
    public init(minimumDistance: CGFloat = 10, coordinateSpace: CoordinateSpace = .local) {
        self.minimumDistance = minimumDistance; self.coordinateSpace = coordinateSpace
    }
    public func _recognizer(_ id: String) -> _GestureRecognizer<Value> { _DragR(minimumDistance, coordinateSpace) }
}

public struct MagnifyGesture: Gesture {
    public struct Value: Equatable {
        public var time: Date
        public var magnification: CGFloat
        public var velocity: CGFloat
        public var startAnchor: UnitPoint
        public var startLocation: CGPoint
    }
    public typealias Body = Never
    public var minimumScaleDelta: CGFloat
    public init(minimumScaleDelta: CGFloat = 0.01) { self.minimumScaleDelta = minimumScaleDelta }
    public func _recognizer(_ id: String) -> _GestureRecognizer<Value> {
        _WrapR(_TwoFingerR(threshold: Double(minimumScaleDelta), rotation: false)) {
            Value(time: _date($0.time), magnification: $0.scale, velocity: $0.scaleVelocity, startAnchor: $0.startAnchor, startLocation: $0.startLocation)
        }
    }
}

@available(iOS, deprecated: 17.0, renamed: "MagnifyGesture")
public struct MagnificationGesture: Gesture {
    public typealias Value = CGFloat
    public typealias Body = Never
    public var minimumScaleDelta: CGFloat
    public init(minimumScaleDelta: CGFloat = 0.01) { self.minimumScaleDelta = minimumScaleDelta }
    public func _recognizer(_ id: String) -> _GestureRecognizer<CGFloat> {
        _WrapR(_TwoFingerR(threshold: Double(minimumScaleDelta), rotation: false)) { $0.scale }
    }
}

public struct RotateGesture: Gesture {
    public struct Value: Equatable {
        public var time: Date
        public var rotation: Angle
        public var velocity: Angle
        public var startAnchor: UnitPoint
        public var startLocation: CGPoint
    }
    public typealias Body = Never
    public var minimumAngleDelta: Angle
    public init(minimumAngleDelta: Angle = .degrees(1)) { self.minimumAngleDelta = minimumAngleDelta }
    public func _recognizer(_ id: String) -> _GestureRecognizer<Value> {
        _WrapR(_TwoFingerR(threshold: abs(minimumAngleDelta.radians), rotation: true)) {
            Value(time: _date($0.time), rotation: .radians($0.angle), velocity: .radians($0.angleVelocity), startAnchor: $0.startAnchor, startLocation: $0.startLocation)
        }
    }
}

@available(iOS, deprecated: 17.0, renamed: "RotateGesture")
public struct RotationGesture: Gesture {
    public typealias Value = Angle
    public typealias Body = Never
    public var minimumAngleDelta: Angle
    public init(minimumAngleDelta: Angle = .degrees(1)) { self.minimumAngleDelta = minimumAngleDelta }
    public func _recognizer(_ id: String) -> _GestureRecognizer<Angle> {
        _WrapR(_TwoFingerR(threshold: abs(minimumAngleDelta.radians), rotation: true)) { .radians($0.angle) }
    }
}

// MARK: - Composition

public struct SimultaneousGesture<First: Gesture, Second: Gesture>: Gesture {
    public struct Value {
        public var first: First.Value?
        public var second: Second.Value?
    }
    public typealias Body = Never
    public var first: First, second: Second
    public init(_ first: First, _ second: Second) { self.first = first; self.second = second }
    public func _recognizer(_ id: String) -> _GestureRecognizer<Value> { _SimultaneousR<First, Second>(first._recognizer(id + ".0"), second._recognizer(id + ".1")) }
}
extension SimultaneousGesture.Value: Equatable where First.Value: Equatable, Second.Value: Equatable {}

public struct SequenceGesture<First: Gesture, Second: Gesture>: Gesture {
    public enum Value {
        case first(First.Value)
        case second(First.Value, Second.Value?)
    }
    public typealias Body = Never
    public var first: First, second: Second
    public init(_ first: First, _ second: Second) { self.first = first; self.second = second }
    public func _recognizer(_ id: String) -> _GestureRecognizer<Value> { _SequenceR<First, Second>(first._recognizer(id + ".0"), second._recognizer(id + ".1")) }
}
extension SequenceGesture.Value: Equatable where First.Value: Equatable, Second.Value: Equatable {}

public struct ExclusiveGesture<First: Gesture, Second: Gesture>: Gesture {
    public enum Value {
        case first(First.Value)
        case second(Second.Value)
    }
    public typealias Body = Never
    public var first: First, second: Second
    public init(_ first: First, _ second: Second) { self.first = first; self.second = second }
    public func _recognizer(_ id: String) -> _GestureRecognizer<Value> { _ExclusiveR<First, Second>(first._recognizer(id + ".0"), second._recognizer(id + ".1")) }
}
extension ExclusiveGesture.Value: Equatable where First.Value: Equatable, Second.Value: Equatable {}

public struct AnyGesture<Value>: Gesture {
    public typealias Body = Never
    let make: (String) -> _GestureRecognizer<Value>
    public init<T: Gesture>(_ gesture: T) where T.Value == Value { make = gesture._recognizer }
    public func _recognizer(_ id: String) -> _GestureRecognizer<Value> { make(id) }
}

public struct GestureStateGesture<Base: Gesture, State>: Gesture {
    public typealias Value = Base.Value
    public typealias Body = Never
    public var base: Base
    public var state: GestureState<State>
    let update: (Value, inout State, inout Transaction) -> Void
    public init(base: Base, state: GestureState<State>, body: @escaping (Value, inout State, inout Transaction) -> Void) {
        self.base = base; self.state = state; self.update = body
    }
    public func _recognizer(_ id: String) -> _GestureRecognizer<Value> { _UpdatingR(base._recognizer(id), state, update) }
}

public struct _ChangedGesture<Content: Gesture>: Gesture {
    public typealias Value = Content.Value
    public typealias Body = Never
    let content: Content, action: (Value) -> Void
    public func _recognizer(_ id: String) -> _GestureRecognizer<Value> { _ChangedR(content._recognizer(id), action) }
}

public struct _EndedGesture<Content: Gesture>: Gesture {
    public typealias Value = Content.Value
    public typealias Body = Never
    let content: Content, action: (Value) -> Void
    public func _recognizer(_ id: String) -> _GestureRecognizer<Value> { _EndedR(content._recognizer(id), action) }
}

public struct _MapGesture<Content: Gesture, Value>: Gesture {
    public typealias Body = Never
    let content: Content, transform: (Content.Value) -> Value
    public func _recognizer(_ id: String) -> _GestureRecognizer<Value> { _WrapR(content._recognizer(id), transform) }
}

struct _PressingGesture<Content: Gesture>: Gesture where Content.Value == Bool {
    typealias Value = Bool
    typealias Body = Never
    let content: Content, pressing: (Bool) -> Void
    func _recognizer(_ id: String) -> _GestureRecognizer<Bool> { _PressingR(content._recognizer(id), pressing) }
}

extension Gesture {
    public func onChanged(_ action: @escaping (Value) -> Void) -> _ChangedGesture<Self> { _ChangedGesture(content: self, action: action) }
    public func onEnded(_ action: @escaping (Value) -> Void) -> _EndedGesture<Self> { _EndedGesture(content: self, action: action) }
    public func map<T>(_ body: @escaping (Value) -> T) -> _MapGesture<Self, T> { _MapGesture(content: self, transform: body) }
    public func updating<State>(_ state: GestureState<State>, body: @escaping (Value, inout State, inout Transaction) -> Void) -> GestureStateGesture<Self, State> {
        GestureStateGesture(base: self, state: state, body: body)
    }
    public func simultaneously<Other: Gesture>(with other: Other) -> SimultaneousGesture<Self, Other> { SimultaneousGesture(self, other) }
    public func sequenced<Other: Gesture>(before other: Other) -> SequenceGesture<Self, Other> { SequenceGesture(self, other) }
    public func exclusively<Other: Gesture>(before other: Other) -> ExclusiveGesture<Self, Other> { ExclusiveGesture(self, other) }
}

// MARK: - @GestureState

/// Gesture-driven state: `updating` sets it while the gesture runs, and it returns to its initial value when the
/// gesture ends or is cancelled. Bound to the view's identity like `@State`.
@propertyWrapper
public struct GestureState<Value>: _StateProperty {
    final class Storage {
        let initial: Value
        let reset: ((Value, inout Transaction) -> Void)?
        var slot: _Slot<Value>?
        init(_ v: Value, _ reset: ((Value, inout Transaction) -> Void)?) { initial = v; self.reset = reset }
    }
    let storage: Storage
    public init(wrappedValue: Value) { storage = Storage(wrappedValue, nil) }
    public init(initialValue: Value) { storage = Storage(initialValue, nil) }
    public init(wrappedValue: Value, resetTransaction: Transaction) { storage = Storage(wrappedValue, nil) }
    public init(initialValue: Value, resetTransaction: Transaction) { storage = Storage(initialValue, nil) }
    public init(wrappedValue: Value, reset: @escaping (Value, inout Transaction) -> Void) { storage = Storage(wrappedValue, reset) }
    public init(initialValue: Value, reset: @escaping (Value, inout Transaction) -> Void) { storage = Storage(initialValue, reset) }

    func _bind(_ key: String) {
        let rt = _Runtime.shared
        rt.touched.insert(key)
        if let s = rt.slots[key] as? _Slot<Value> { storage.slot = s; return }
        let s = _Slot(storage.initial)
        rt.slots[key] = s
        storage.slot = s
    }

    public var wrappedValue: Value { storage.slot?.value ?? storage.initial }
    public var projectedValue: GestureState<Value> { self }

    func _set(_ v: Value) {
        storage.slot?.value = v
        _Runtime.shared.dirty = true
    }

    func _reset() {
        let old = wrappedValue
        _set(storage.initial)
        var t = Transaction()
        storage.reset?(old, &t)
    }
}

extension GestureState where Value: ExpressibleByNilLiteral {
    public init(resetTransaction: Transaction = Transaction()) { self.init(wrappedValue: nil) }
    public init(reset: @escaping (Value, inout Transaction) -> Void) { self.init(wrappedValue: nil, reset: reset) }
}

// MARK: - Views with gestures

/// How a gesture attached to a view competes with the others under the finger.
enum _GestureKind { case normal, simultaneous, high }

struct _GestureEntry {
    let kind: _GestureKind
    let make: (String) -> _Recognizing
}

/// Register `make` for each node and mark the node with a `gesture` modifier (section L). Each attachment has its own
/// key, so `.gesture(a).gesture(b)` are two targets, `a` (attached first) inner.
func _attachGesture(_ nodes: [Node], _ path: String, _ kind: _GestureKind, _ make: @escaping (String) -> _Recognizing) -> [Node] {
    let rt = _Runtime.shared
    let pans = make("").pans
    return nodes.enumerated().map { i, node in
        var n = node
        var mods = (n["modifiers"] as? [Node]) ?? []
        let count = mods.filter { $0["gesture"] != nil }.count
        let key = "\(path)\(i == 0 ? "" : "~\(i)")#g\(count)"
        rt.gestures[key] = _GestureEntry(kind: kind, make: make)
        mods.append(["gesture": ["key": key, "pans": pans]])
        n["modifiers"] = mods
        return n
    }
}

struct _GestureView<Content: View, G: Gesture>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, gesture: G, kind: _GestureKind, mask: GestureMask
    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared
        let off = rt.gesturesOff
        if !mask.contains(.subviews) { rt.gesturesOff = true }
        let nodes = _modTargets(content, path)
        rt.gesturesOff = off
        guard !off && mask.contains(.gesture) else { return nodes }
        let g = gesture
        return _attachGesture(nodes, path, kind) { g._recognizer($0) }
    }
}

extension View {
    public func gesture<T: Gesture>(_ gesture: T, including mask: GestureMask = .all) -> some View {
        _GestureView(content: self, gesture: gesture, kind: .normal, mask: mask)
    }
    public func gesture<T: Gesture>(_ gesture: T, isEnabled: Bool) -> some View {
        _GestureView(content: self, gesture: gesture, kind: .normal, mask: isEnabled ? .all : .subviews)
    }
    public func simultaneousGesture<T: Gesture>(_ gesture: T, including mask: GestureMask = .all) -> some View {
        _GestureView(content: self, gesture: gesture, kind: .simultaneous, mask: mask)
    }
    public func highPriorityGesture<T: Gesture>(_ gesture: T, including mask: GestureMask = .all) -> some View {
        _GestureView(content: self, gesture: gesture, kind: .high, mask: mask)
    }

    public func onLongPressGesture(minimumDuration: Double = 0.5, maximumDistance: CGFloat = 10, perform action: @escaping () -> Void,
                                   onPressingChanged: ((Bool) -> Void)? = nil) -> some View {
        let press = LongPressGesture(minimumDuration: minimumDuration, maximumDistance: maximumDistance)
        return gesture(_PressingGesture(content: press, pressing: onPressingChanged ?? { _ in }).onEnded { _ in action() })
    }
    public func onLongPressGesture(minimumDuration: Double = 0.5, maximumDistance: CGFloat = 10, pressing: ((Bool) -> Void)?,
                                   perform action: @escaping () -> Void) -> some View {
        onLongPressGesture(minimumDuration: minimumDuration, maximumDistance: maximumDistance, perform: action, onPressingChanged: pressing)
    }

    public func onTapGesture(count: Int = 1, coordinateSpace: CoordinateSpace = .local, perform action: @escaping (CGPoint) -> Void) -> some View {
        gesture(SpatialTapGesture(count: count, coordinateSpace: coordinateSpace).onEnded { action($0.location) })
    }

    public func offset(_ offset: CGSize) -> some View { self.offset(x: offset.width, y: offset.height) }
    public func offset(x: CGFloat = 0, y: CGFloat = 0) -> some View { _modified { ["offset": ["x": Double(x), "y": Double(y)]] } }
}

// MARK: - Touch handling

/// The gestures under one touch sequence and which of them has won.
final class _TouchSession {
    struct Candidate { let kind: _GestureKind; let key: String; let r: _Recognizing }
    var candidates: [Candidate] = []
    /// Index into `candidates` of the exclusive gesture that won.
    var winner: Int?

    /// Exclusive candidates in priority order: high-priority gestures outermost first, then the rest innermost first.
    var exclusive: [Int] {
        let i = candidates.indices
        return i.filter { candidates[$0].kind == .high }.reversed() + i.filter { candidates[$0].kind == .normal }
    }
}

func _decided(_ s: _Status) -> Bool { s == .active || s == .ended }

/// One host touch event (section L): `targets` are (key, frame) innermost first.
func _handleTouch(_ phase: _Touch.Phase, _ time: Double, _ points: [CGPoint], _ targets: [(String, CGPoint, CGSize)]) {
    let rt = _Runtime.shared
    if phase == .start {
        let s = _TouchSession()
        for (key, _, _) in targets {
            if let e = rt.gestures[key] { s.candidates.append(.init(kind: e.kind, key: key, r: e.make(key))) }
        }
        rt.touch = s
    }
    // Every touch event answers with a tree, so the host (and tests) can wait for exactly one.
    guard let s = rt.touch else { return rt.submitTree() }
    let frames = Dictionary(targets.map { ($0.0, ($0.1, $0.2)) }, uniquingKeysWith: { a, _ in a })
    let queues = s.candidates.map { c -> _Queue in
        let q = _Queue()
        let f = frames[c.key] ?? (CGPoint(), CGSize())
        c.r.feed(_Touch(phase: phase, time: time, points: points, origin: f.0, size: f.1), q)
        return q
    }
    let order = s.exclusive
    var run: Set<Int> = Set(s.candidates.indices.filter { s.candidates[$0].kind == .simultaneous })
    if s.winner == nil {
        // The first exclusive gesture to commit wins (higher priority first when several commit on one event); the
        // others are cancelled. Until then, undecided gestures show their progress (a long press's pressing state).
        s.winner = order.first { _decided(s.candidates[$0].r.status) }
        if let w = s.winner {
            for i in order where i != w { s.candidates[i].r.cancel(queues[i]) }
        } else {
            run.formUnion(order.filter { s.candidates[$0].r.status != .failed })
        }
    }
    if let w = s.winner { run.insert(w) }
    _pool {
        for i in s.candidates.indices where run.contains(i) { queues[i].actions.forEach { $0() } }
        for q in queues { q.resets.forEach { $0() } }
    }
    if phase == .end || phase == .cancel {
        // Anything still undecided lost with the touch.
        let tail = _Queue()
        for c in s.candidates { c.r.cancel(tail) }
        _pool { tail.resets.forEach { $0() } }
        rt.touch = nil
    }
    rt.submitTree()
}
