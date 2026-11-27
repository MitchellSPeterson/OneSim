#if os(WASI)
import WASILibc
#else
// Apple's SwiftUI re-exports Foundation (`Date`, `TimeInterval` in `TimelineView`, `Spring`, `Animation`).
@_exported import Foundation
#endif

// Animation, transitions, matchedGeometryEffect, contentTransition, PhaseAnimator, KeyframeAnimator,
// and TimelineView (docs/protocol.md section P).

// MARK: - VectorArithmetic & Animatable

public protocol VectorArithmetic: AdditiveArithmetic {
    mutating func scale(by rhs: Double)
    var magnitudeSquared: Double { get }
}

extension VectorArithmetic {
    public func scaled(by rhs: Double) -> Self {
        var copy = self
        copy.scale(by: rhs)
        return copy
    }

    public mutating func interpolate(towards other: Self, amount: Double) {
        self += (other - self).scaled(by: amount)
    }

    public func interpolated(towards other: Self, amount: Double) -> Self {
        var copy = self
        copy.interpolate(towards: other, amount: amount)
        return copy
    }
}

public struct EmptyAnimatableData: VectorArithmetic, Hashable, Sendable {
    public init() {}
    public static var zero: EmptyAnimatableData { EmptyAnimatableData() }
    public static func + (lhs: Self, rhs: Self) -> Self { .zero }
    public static func - (lhs: Self, rhs: Self) -> Self { .zero }
    public mutating func scale(by rhs: Double) {}
    public var magnitudeSquared: Double { 0 }
}

public struct AnimatablePair<First: VectorArithmetic, Second: VectorArithmetic>: VectorArithmetic {
    public var first: First
    public var second: Second
    public init(_ first: First, _ second: Second) { self.first = first; self.second = second }
    public static var zero: Self { Self(.zero, .zero) }
    public static func + (lhs: Self, rhs: Self) -> Self { Self(lhs.first + rhs.first, lhs.second + rhs.second) }
    public static func - (lhs: Self, rhs: Self) -> Self { Self(lhs.first - rhs.first, lhs.second - rhs.second) }
    public mutating func scale(by rhs: Double) { first.scale(by: rhs); second.scale(by: rhs) }
    public var magnitudeSquared: Double { first.magnitudeSquared + second.magnitudeSquared }
}

extension Double: VectorArithmetic {
    public mutating func scale(by rhs: Double) { self *= rhs }
    public var magnitudeSquared: Double { self * self }
}

extension Float: VectorArithmetic {
    public mutating func scale(by rhs: Double) { self = Float(Double(self) * rhs) }
    public var magnitudeSquared: Double { Double(self) * Double(self) }
}

#if !os(WASI)
extension CGFloat: VectorArithmetic {
    public mutating func scale(by rhs: Double) { self = CGFloat(Double(self) * rhs) }
    public var magnitudeSquared: Double { Double(self) * Double(self) }
}
#endif

public protocol Animatable {
    associatedtype AnimatableData: VectorArithmetic = EmptyAnimatableData
    var animatableData: Self.AnimatableData { get set }
}

extension Animatable where AnimatableData == EmptyAnimatableData {
    public var animatableData: EmptyAnimatableData {
        get { .zero }
        set {}
    }
}

extension Double: Animatable {
    public var animatableData: Double { get { self } set { self = newValue } }
}
extension Float: Animatable {
    public var animatableData: Float { get { self } set { self = newValue } }
}
#if !os(WASI)
extension CGFloat: Animatable {
    public var animatableData: CGFloat { get { self } set { self = newValue } }
}
#endif
extension CGPoint: Animatable {
    public var animatableData: AnimatablePair<CGFloat, CGFloat> {
        get { AnimatablePair(x, y) }
        set { x = newValue.first; y = newValue.second }
    }
}
extension CGSize: Animatable {
    public var animatableData: AnimatablePair<CGFloat, CGFloat> {
        get { AnimatablePair(width, height) }
        set { width = newValue.first; height = newValue.second }
    }
}
extension CGRect: Animatable {
    public var animatableData: AnimatablePair<AnimatablePair<CGFloat, CGFloat>, AnimatablePair<CGFloat, CGFloat>> {
        get { AnimatablePair(origin.animatableData, size.animatableData) }
        set { origin.animatableData = newValue.first; size.animatableData = newValue.second }
    }
}
extension Angle: Animatable {
    public var animatableData: Double {
        get { radians }
        set { radians = newValue }
    }
}

// MARK: - UnitCurve

public struct UnitCurve: Equatable, Hashable, Sendable {
    let x1: Double, y1: Double, x2: Double, y2: Double

    public static let linear = UnitCurve(x1: 0, y1: 0, x2: 1, y2: 1)
    public static let easeIn = UnitCurve(x1: 0.42, y1: 0, x2: 1, y2: 1)
    public static let easeOut = UnitCurve(x1: 0, y1: 0, x2: 0.58, y2: 1)
    public static let easeInOut = UnitCurve(x1: 0.42, y1: 0, x2: 0.58, y2: 1)
    public static let circularEaseIn = UnitCurve(x1: 0.55, y1: 0, x2: 1, y2: 0.45)
    public static let circularEaseOut = UnitCurve(x1: 0, y1: 0.55, x2: 0.45, y2: 1)
    public static let circularEaseInOut = UnitCurve(x1: 0.85, y1: 0, x2: 0.15, y2: 1)

    public static func bezier(startControlPoint: UnitPoint, endControlPoint: UnitPoint) -> UnitCurve {
        UnitCurve(x1: Double(startControlPoint.x), y1: Double(startControlPoint.y),
                  x2: Double(endControlPoint.x), y2: Double(endControlPoint.y))
    }

    public var inverse: UnitCurve {
        UnitCurve(x1: 1 - x2, y1: 1 - y2, x2: 1 - x1, y2: 1 - y1)
    }

    public func value(at progress: Double) -> Double {
        if progress <= 0 { return 0 }
        if progress >= 1 { return 1 }
        if self == .linear { return progress }
        let s = _solveBezierX(progress)
        return _sampleBezier(y1, y2, s)
    }

    public func velocity(at progress: Double) -> Double {
        if self == .linear { return 1 }
        let u = min(max(progress, 0), 1)
        let s = _solveBezierX(u)
        let dx = max(_derivBezier(x1, x2, s), 1e-6)
        return _derivBezier(y1, y2, s) / dx
    }

    private func _sampleBezier(_ a: Double, _ b: Double, _ s: Double) -> Double {
        let ax = 3 * a - 3 * b + 1
        let bx = 3 * b - 6 * a
        let cx = 3 * a
        return ((ax * s + bx) * s + cx) * s
    }

    private func _derivBezier(_ a: Double, _ b: Double, _ s: Double) -> Double {
        let ax = 3 * a - 3 * b + 1
        let bx = 3 * b - 6 * a
        let cx = 3 * a
        return (3 * ax * s + 2 * bx) * s + cx
    }

    private func _solveBezierX(_ u: Double) -> Double {
        var s = u, lo = 0.0, hi = 1.0
        for _ in 0..<8 {
            let x = _sampleBezier(x1, x2, s) - u
            if abs(x) < 1e-6 { break }
            if x < 0 { lo = s } else { hi = s }
            let d = _derivBezier(x1, x2, s)
            if abs(d) > 1e-6 {
                let next = s - x / d
                s = (next > lo && next < hi) ? next : 0.5 * (lo + hi)
            } else {
                s = 0.5 * (lo + hi)
            }
        }
        return s
    }
}

// MARK: - Spring

public struct Spring: Equatable, Hashable, Sendable {
    public let mass: Double
    public let stiffness: Double
    public let damping: Double

    public init(mass: Double = 1.0, stiffness: Double, damping: Double, allowOverDamping: Bool = false) {
        self.mass = max(mass, 1e-4)
        self.stiffness = max(stiffness, 1e-4)
        let crit = 2.0 * (self.mass * self.stiffness).squareRoot()
        self.damping = allowOverDamping ? max(damping, 0) : min(max(damping, 0), crit)
    }

    public init(response: Double, dampingRatio: Double) {
        let r = max(response, 1e-3)
        let k = pow(2.0 * .pi / r, 2.0)
        let c = 4.0 * .pi * max(dampingRatio, 0) / r
        self.mass = 1.0
        self.stiffness = k
        self.damping = c
    }

    public init(duration: TimeInterval = 0.5, bounce: Double = 0.0) {
        let d = max(duration, 1e-3)
        let b = min(max(bounce, -1.0), 1.0)
        let zeta = b >= 0 ? 1.0 - b : 1.0 / (1.0 + b)
        let k = pow(2.0 * .pi / d, 2.0)
        let c = 2.0 * zeta * k.squareRoot()
        self.mass = 1.0
        self.stiffness = k
        self.damping = c
    }

    public init(settlingDuration: TimeInterval, dampingRatio: Double, epsilon: Double = 0.001) {
        let dur = max(settlingDuration, 1e-3)
        let zeta = max(dampingRatio, 1e-3)
        let w0 = -log(max(epsilon, 1e-6)) / (zeta * dur)
        self.mass = 1.0
        self.stiffness = w0 * w0
        self.damping = 2.0 * zeta * w0
    }

    public var dampingRatio: Double { damping / (2.0 * (stiffness * mass).squareRoot()) }
    public var response: Double { 2.0 * .pi / (stiffness / mass).squareRoot() }
    public var duration: TimeInterval { response }
    public var bounce: Double {
        let z = dampingRatio
        return z <= 1.0 ? 1.0 - z : (1.0 / z) - 1.0
    }
    public var settlingDuration: TimeInterval {
        let w0 = (stiffness / mass).squareRoot()
        let z = max(min(dampingRatio, 1.0), 0.05)
        return min(max(-log(0.001) / (z * w0), response), 4.0)
    }

    public static var smooth: Spring { Spring(duration: 0.5, bounce: 0.0) }
    public static func smooth(duration: TimeInterval = 0.5, extraBounce: Double = 0.0) -> Spring {
        Spring(duration: duration, bounce: extraBounce)
    }
    public static var snappy: Spring { Spring(duration: 0.5, bounce: 0.15) }
    public static func snappy(duration: TimeInterval = 0.5, extraBounce: Double = 0.0) -> Spring {
        Spring(duration: duration, bounce: 0.15 + extraBounce)
    }
    public static var bouncy: Spring { Spring(duration: 0.5, bounce: 0.3) }
    public static func bouncy(duration: TimeInterval = 0.5, extraBounce: Double = 0.0) -> Spring {
        Spring(duration: duration, bounce: 0.3 + extraBounce)
    }

    func _evalScalar(target: Double, initialVelocity: Double, time t: TimeInterval) -> (Double, Double) {
        if t <= 0 { return (0, initialVelocity) }
        let w0 = (stiffness / mass).squareRoot()
        let zeta = dampingRatio
        let a = -target
        let v0 = initialVelocity
        if zeta < 1.0 - 1e-5 {
            let wd = w0 * (1.0 - zeta * zeta).squareRoot()
            let decay = exp(-zeta * w0 * t)
            let b = (v0 + zeta * w0 * a) / wd
            let c = cos(wd * t), s = sin(wd * t)
            let x = decay * (a * c + b * s)
            let v = decay * ((b * wd - zeta * w0 * a) * c - (a * wd + zeta * w0 * b) * s)
            return (target + x, v)
        } else if zeta > 1.0 + 1e-5 {
            let root = w0 * (zeta * zeta - 1.0).squareRoot()
            let r1 = -zeta * w0 + root
            let r2 = -zeta * w0 - root
            let c1 = (v0 - r2 * a) / (r1 - r2)
            let c2 = a - c1
            let e1 = exp(r1 * t), e2 = exp(r2 * t)
            return (target + c1 * e1 + c2 * e2, c1 * r1 * e1 + c2 * r2 * e2)
        } else {
            let decay = exp(-w0 * t)
            let b = v0 + w0 * a
            let x = decay * (a + b * t)
            let v = decay * (b - w0 * (a + b * t))
            return (target + x, v)
        }
    }

    public func value(target: Double, initialVelocity: Double = 0, time: TimeInterval) -> Double {
        _evalScalar(target: target, initialVelocity: initialVelocity, time: time).0
    }

    public func velocity(target: Double, initialVelocity: Double = 0, time: TimeInterval) -> Double {
        _evalScalar(target: target, initialVelocity: initialVelocity, time: time).1
    }

    public func value<V: VectorArithmetic>(target: V, initialVelocity: V = .zero, time: TimeInterval) -> V {
        let p = _evalScalar(target: 1.0, initialVelocity: 0.0, time: time).0
        let (vp, _) = _evalScalar(target: 0.0, initialVelocity: 1.0, time: time)
        return target.scaled(by: p) + initialVelocity.scaled(by: vp)
    }

    public func velocity<V: VectorArithmetic>(target: V, initialVelocity: V = .zero, time: TimeInterval) -> V {
        let v1 = _evalScalar(target: 1.0, initialVelocity: 0.0, time: time).1
        let v2 = _evalScalar(target: 0.0, initialVelocity: 1.0, time: time).1
        return target.scaled(by: v1) + initialVelocity.scaled(by: v2)
    }

    public func update<V: VectorArithmetic>(value: inout V, velocity: inout V, target: V, deltaTime: TimeInterval) {
        let delta = target - value
        let newOffset = self.value(target: delta, initialVelocity: velocity, time: deltaTime)
        let newVel = self.velocity(target: delta, initialVelocity: velocity, time: deltaTime)
        value += newOffset
        velocity = newVel
    }
}

// MARK: - CustomAnimation, AnimationState, AnimationContext

public enum AnimationCompletionCriteria: Equatable, Hashable, Sendable {
    case logicallyComplete
    case removed
}

public protocol AnimationStateKey {
    associatedtype Value
    static var defaultValue: Self.Value { get }
}

public struct AnimationState<Value: VectorArithmetic> {
    var storage: [ObjectIdentifier: Any] = [:]
    public init() {}
    public subscript<K: AnimationStateKey>(key: K.Type) -> K.Value {
        get { (storage[ObjectIdentifier(key)] as? K.Value) ?? K.defaultValue }
        set { storage[ObjectIdentifier(key)] = newValue }
    }
}

public struct AnimationContext<Value: VectorArithmetic> {
    public var state: AnimationState<Value>
    public var isLogicallyComplete: Bool = false
    public init(state: AnimationState<Value> = AnimationState()) { self.state = state }
}

public protocol CustomAnimation: Hashable {
    func animate<V: VectorArithmetic>(value: V, time: TimeInterval, context: inout AnimationContext<V>) -> V?
    func shouldMerge<V: VectorArithmetic>(previous: Animation, value: V, time: TimeInterval, context: inout AnimationContext<V>) -> Bool
}

extension CustomAnimation {
    public func shouldMerge<V: VectorArithmetic>(previous: Animation, value: V, time: TimeInterval, context: inout AnimationContext<V>) -> Bool {
        false
    }
}

// MARK: - Animation

public struct Animation: Equatable {
    enum Curve: Equatable {
        case linear
        case easeIn
        case easeOut
        case easeInOut
        case bezier(Double, Double, Double, Double)
        case spring(mass: Double, stiffness: Double, damping: Double, initialVelocity: Double, settling: Double)
        case samples([Double])
    }
    enum Repeat: Equatable {
        case count(Int)
        case forever
    }

    var curve: Curve
    var duration: Double
    var delayTime: Double = 0
    var speedFactor: Double = 1
    var repeatMode: Repeat? = nil
    var autoreverses: Bool = true

    init(curve: Curve, duration: Double) {
        self.curve = curve
        self.duration = duration
    }

    public static var `default`: Animation { .spring(duration: 0.35, bounce: 0.0) }

    public static var linear: Animation { .linear(duration: 0.35) }
    public static func linear(duration: TimeInterval) -> Animation { Animation(curve: .linear, duration: max(duration, 0)) }

    public static var easeIn: Animation { .easeIn(duration: 0.35) }
    public static func easeIn(duration: TimeInterval) -> Animation { Animation(curve: .easeIn, duration: max(duration, 0)) }

    public static var easeOut: Animation { .easeOut(duration: 0.35) }
    public static func easeOut(duration: TimeInterval) -> Animation { Animation(curve: .easeOut, duration: max(duration, 0)) }

    public static var easeInOut: Animation { .easeInOut(duration: 0.35) }
    public static func easeInOut(duration: TimeInterval) -> Animation { Animation(curve: .easeInOut, duration: max(duration, 0)) }

    public static func timingCurve(_ c0x: Double, _ c0y: Double, _ c1x: Double, _ c1y: Double, duration: TimeInterval = 0.35) -> Animation {
        Animation(curve: .bezier(c0x, c0y, c1x, c1y), duration: max(duration, 0))
    }

    public static func timingCurve(_ curve: UnitCurve, duration: TimeInterval = 0.35) -> Animation {
        Animation(curve: .bezier(curve.x1, curve.y1, curve.x2, curve.y2), duration: max(duration, 0))
    }

    public static var spring: Animation { .spring(duration: 0.5, bounce: 0.0) }
    public static func spring(_ spring: Spring, blendDuration: TimeInterval = 0) -> Animation {
        let _ = blendDuration
        return Animation(
            curve: .spring(mass: spring.mass, stiffness: spring.stiffness, damping: spring.damping, initialVelocity: 0, settling: spring.settlingDuration),
            duration: spring.settlingDuration
        )
    }
    public static func spring(duration: TimeInterval = 0.5, bounce: Double = 0.0, blendDuration: TimeInterval = 0) -> Animation {
        .spring(Spring(duration: duration, bounce: bounce), blendDuration: blendDuration)
    }
    public static func spring(response: Double = 0.55, dampingFraction: Double = 0.825, blendDuration: TimeInterval = 0) -> Animation {
        .spring(Spring(response: response, dampingRatio: dampingFraction), blendDuration: blendDuration)
    }
    public static func interactiveSpring(response: Double = 0.15, dampingFraction: Double = 0.86, blendDuration: TimeInterval = 0.25) -> Animation {
        .spring(Spring(response: response, dampingRatio: dampingFraction), blendDuration: blendDuration)
    }
    public static func interactiveSpring(duration: TimeInterval = 0.15, extraBounce: Double = 0.0, blendDuration: TimeInterval = 0.25) -> Animation {
        .spring(Spring(duration: duration, bounce: extraBounce), blendDuration: blendDuration)
    }
    public static func interpolatingSpring(mass: Double = 1.0, stiffness: Double, damping: Double, initialVelocity: Double = 0.0) -> Animation {
        let s = Spring(mass: mass, stiffness: stiffness, damping: damping, allowOverDamping: true)
        return Animation(
            curve: .spring(mass: s.mass, stiffness: s.stiffness, damping: s.damping, initialVelocity: initialVelocity, settling: s.settlingDuration),
            duration: s.settlingDuration
        )
    }
    public static func interpolatingSpring(duration: TimeInterval = 0.5, bounce: Double = 0.0, initialVelocity: Double = 0.0) -> Animation {
        let s = Spring(duration: duration, bounce: bounce)
        return Animation(
            curve: .spring(mass: s.mass, stiffness: s.stiffness, damping: s.damping, initialVelocity: initialVelocity, settling: s.settlingDuration),
            duration: s.settlingDuration
        )
    }
    public static var smooth: Animation { .spring(.smooth) }
    public static func smooth(duration: TimeInterval = 0.5, extraBounce: Double = 0.0) -> Animation {
        .spring(.smooth(duration: duration, extraBounce: extraBounce))
    }
    public static var snappy: Animation { .spring(.snappy) }
    public static func snappy(duration: TimeInterval = 0.5, extraBounce: Double = 0.0) -> Animation {
        .spring(.snappy(duration: duration, extraBounce: extraBounce))
    }
    public static var bouncy: Animation { .spring(.bouncy) }
    public static func bouncy(duration: TimeInterval = 0.5, extraBounce: Double = 0.0) -> Animation {
        .spring(.bouncy(duration: duration, extraBounce: extraBounce))
    }

    public init<A: CustomAnimation>(_ base: A) {
        // Sample the custom animation over up to 2 s in 16 ms steps to determine duration & curve.
        var ctx = AnimationContext<Double>()
        var pts: [Double] = [0.0]
        var endTime = 0.0
        let dt = 0.016
        for i in 1...125 {
            let t = Double(i) * dt
            if let v = base.animate(value: 1.0, time: t, context: &ctx) {
                pts.append(v)
                endTime = t
            } else {
                pts.append(1.0)
                if endTime == 0 { endTime = t }
                break
            }
        }
        self.curve = .samples(pts)
        self.duration = max(endTime, 0.016)
    }

    public func delay(_ delay: TimeInterval) -> Animation {
        var copy = self
        copy.delayTime += max(delay, 0)
        return copy
    }

    public func speed(_ speed: Double) -> Animation {
        var copy = self
        copy.speedFactor *= max(speed, 1e-6)
        return copy
    }

    public func repeatCount(_ repeatCount: Int, autoreverses: Bool = true) -> Animation {
        var copy = self
        copy.repeatMode = .count(max(repeatCount, 0))
        copy.autoreverses = autoreverses
        return copy
    }

    public func repeatForever(autoreverses: Bool = true) -> Animation {
        var copy = self
        copy.repeatMode = .forever
        copy.autoreverses = autoreverses
        return copy
    }

    /// Logical duration of one run (seconds), used by `PhaseAnimator` and completion callbacks.
    var _totalDuration: Double {
        let base: Double
        if case .spring(_, _, _, _, let settling) = curve { base = settling } else { base = duration }
        let reps: Double
        switch repeatMode {
        case .none: reps = 1
        case .count(let n): reps = Double(max(n, 1))
        case .forever: reps = 1
        }
        return delayTime + (base * reps) / max(speedFactor, 1e-6)
    }

    func _wire(token: UInt64) -> Node {
        let c: Any
        switch curve {
        case .linear: c = "linear"
        case .easeIn: c = "easeIn"
        case .easeOut: c = "easeOut"
        case .easeInOut: c = "easeInOut"
        case .bezier(let x1, let y1, let x2, let y2): c = ["bezier": [x1, y1, x2, y2]]
        case .spring(let m, let k, let d, let v0, let s):
            c = ["spring": ["mass": m, "stiffness": k, "damping": d, "initialVelocity": v0, "settling": s]]
        case .samples(let pts): c = ["samples": pts]
        }
        var j: Node = ["curve": c, "duration": duration, "delay": delayTime, "speed": speedFactor, "autoreverses": autoreverses, "token": token]
        if let r = repeatMode {
            switch r {
            case .count(let n): j["repeat"] = n
            case .forever: j["repeat"] = "forever"
            }
        }
        return j
    }
}

// MARK: - Transaction & TransactionKey

public protocol TransactionKey {
    associatedtype Value
    static var defaultValue: Self.Value { get }
}

public struct Transaction {
    public var animation: Animation? = nil
    public var disablesAnimations: Bool = false
    public var isContinuous: Bool = false
    public var tracksVelocity: Bool = false
    var custom: [ObjectIdentifier: Any] = [:]
    var completions: [() -> Void] = []

    public init() {}
    public init(animation: Animation?) { self.animation = animation }

    public subscript<K: TransactionKey>(key: K.Type) -> K.Value {
        get { (custom[ObjectIdentifier(key)] as? K.Value) ?? K.defaultValue }
        set { custom[ObjectIdentifier(key)] = newValue }
    }

    public mutating func addAnimationCompletion(criteria: AnimationCompletionCriteria = .logicallyComplete, _ completion: @escaping () -> Void) {
        let _ = criteria
        completions.append(completion)
    }
}

public func withTransaction<Result>(_ transaction: Transaction, _ body: () throws -> Result) rethrows -> Result {
    let rt = _Runtime.shared
    let saved = rt.currentTransaction
    rt.currentTransaction = transaction
    defer { rt.currentTransaction = saved }
    let result = try body()
    // If nothing inside `withTransaction` drained the completions via `markDirty()`, run them immediately.
    if let rem = rt.currentTransaction?.completions, !rem.isEmpty {
        rt.currentTransaction?.completions.removeAll()
        for fn in rem { fn() }
    }
    return result
}

public func withTransaction<V, Result>(_ keyPath: WritableKeyPath<Transaction, V>, _ value: V, _ body: () throws -> Result) rethrows -> Result {
    var tx = _Runtime.shared.currentTransaction ?? Transaction()
    tx[keyPath: keyPath] = value
    return try withTransaction(tx, body)
}

public func withAnimation<Result>(_ animation: Animation? = .default, _ body: () throws -> Result) rethrows -> Result {
    try withTransaction(Transaction(animation: animation), body)
}

public func withAnimation<Result>(
    _ animation: Animation? = .default,
    completionCriteria: AnimationCompletionCriteria = .logicallyComplete,
    _ body: () throws -> Result,
    completion: @escaping () -> Void
) rethrows -> Result {
    var tx = Transaction(animation: animation)
    tx.addAnimationCompletion(criteria: completionCriteria, completion)
    return try withTransaction(tx, body)
}

// MARK: - .animation & .transaction View modifiers

struct _AnimationValueView<Content: View, V: Equatable>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, animation: Animation?, value: V
    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared
        let changed = rt.noteAnimValue(path, value)
        let nodes = _modTargets(content, path)
        guard changed else { return nodes }
        let mod: Node = animation.map { ["animation": $0._wire(token: rt.nextAnimToken())] } ?? ["animation": NSNull()]
        return nodes.map { var n = $0; n["modifiers"] = ((n["modifiers"] as? [Node]) ?? []) + [mod]; return n }
    }
}

struct _AnimationImplicitView<Content: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, animation: Animation?
    func _children(_ path: String) -> [Node] {
        let mod: Node = animation.map { ["animation": $0._wire(token: 0)] } ?? ["animation": NSNull()]
        return _modTargets(content, path).map { var n = $0; n["modifiers"] = ((n["modifiers"] as? [Node]) ?? []) + [mod]; return n }
    }
}

private func _applyTransaction(_ tx: Transaction, to nodes: [Node], rt: _Runtime) -> [Node] {
    if tx.disablesAnimations {
        return nodes.map { var n = $0; n["modifiers"] = ((n["modifiers"] as? [Node]) ?? []) + [["animation": NSNull()]]; return n }
    }
    if let anim = tx.animation {
        let mod: Node = ["animation": anim._wire(token: rt.nextAnimToken())]
        return nodes.map { var n = $0; n["modifiers"] = ((n["modifiers"] as? [Node]) ?? []) + [mod]; return n }
    }
    return nodes
}

struct _TransactionView<Content: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, transform: (inout Transaction) -> Void
    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared
        let prevActive = rt.activeTransaction
        var tx = prevActive ?? Transaction()
        transform(&tx)
        rt.activeTransaction = tx
        let nodes = _modTargets(content, path)
        rt.activeTransaction = prevActive
        return _applyTransaction(tx, to: nodes, rt: rt)
    }
}

struct _TransactionValueView<Content: View, V: Equatable>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, value: V, transform: (inout Transaction) -> Void
    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared
        let changed = rt.noteAnimValue(path, value)
        let nodes = _modTargets(content, path)
        guard changed else { return nodes }
        var tx = rt.activeTransaction ?? Transaction()
        transform(&tx)
        return _applyTransaction(tx, to: nodes, rt: rt)
    }
}

extension View {
    public func animation<V: Equatable>(_ animation: Animation?, value: V) -> some View {
        _AnimationValueView(content: self, animation: animation, value: value)
    }

    public func animation(_ animation: Animation?) -> some View {
        _AnimationImplicitView(content: self, animation: animation)
    }

    public func animation<V: View>(_ animation: Animation?, @ViewBuilder body: (PlaceholderContentView<Self>) -> V) -> some View {
        body(PlaceholderContentView(content: self)).animation(animation)
    }

    public func transaction(_ transform: @escaping (inout Transaction) -> Void) -> some View {
        _TransactionView(content: self, transform: transform)
    }

    public func transaction<V: Equatable>(value: V, _ transform: @escaping (inout Transaction) -> Void) -> some View {
        _TransactionValueView(content: self, value: value, transform: transform)
    }
}

// MARK: - PlaceholderContentView

public struct PlaceholderContentView<Value>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let emitChildren: (String) -> [Node]
    init<V: View>(content: V) {
        emitChildren = { path in SwiftUI._children(content, path) }
    }
    init(emitChildren: @escaping (String) -> [Node]) {
        self.emitChildren = emitChildren
    }
    func _children(_ path: String) -> [Node] { emitChildren(path) }
}

// MARK: - Transition & AnyTransition

public enum TransitionPhase: Hashable, Sendable {
    case willAppear
    case identity
    case didDisappear

    public var isIdentity: Bool { self == .identity }
    public var value: Double {
        switch self {
        case .willAppear: return -1
        case .identity: return 0
        case .didDisappear: return 1
        }
    }
}

public struct TransitionProperties: Sendable {
    public var hasMotion: Bool
    public init(hasMotion: Bool = true) { self.hasMotion = hasMotion }
}

struct _TransitionEffect {
    var opacity: Double? = nil
    var scale: Double? = nil
    var blur: Double? = nil
    var anchor: UnitPoint? = nil
    var offsetX: Double? = nil
    var offsetY: Double? = nil
    var edge: Edge? = nil

    func merged(with other: _TransitionEffect) -> _TransitionEffect {
        _TransitionEffect(
            opacity: other.opacity ?? opacity,
            scale: other.scale ?? scale,
            blur: other.blur ?? blur,
            anchor: other.anchor ?? anchor,
            offsetX: (offsetX ?? 0) + (other.offsetX ?? 0) == 0 && offsetX == nil && other.offsetX == nil ? nil : (offsetX ?? 0) + (other.offsetX ?? 0),
            offsetY: (offsetY ?? 0) + (other.offsetY ?? 0) == 0 && offsetY == nil && other.offsetY == nil ? nil : (offsetY ?? 0) + (other.offsetY ?? 0),
            edge: other.edge ?? edge
        )
    }

    func wire() -> Node {
        var j: Node = [:]
        if let opacity { j["opacity"] = opacity }
        if let scale { j["scale"] = scale }
        if let blur { j["blur"] = blur }
        if let anchor { j["anchor"] = [Double(anchor.x), Double(anchor.y)] }
        if offsetX != nil || offsetY != nil { j["offset"] = [offsetX ?? 0, offsetY ?? 0] }
        if let edge { j["edge"] = _edgeName(edge) }
        return j
    }
}

func _edgeName(_ e: Edge) -> String {
    switch e {
    case .top: return "top"
    case .bottom: return "bottom"
    case .leading: return "leading"
    case .trailing: return "trailing"
    }
}

func _oppositeEdge(_ e: Edge) -> Edge {
    switch e {
    case .top: return .bottom
    case .bottom: return .top
    case .leading: return .trailing
    case .trailing: return .leading
    }
}

struct _TransitionSpec {
    var insertion: _TransitionEffect = _TransitionEffect()
    var removal: _TransitionEffect = _TransitionEffect()
    var animation: Animation? = nil

    func wire() -> Node {
        var j: Node = ["insertion": insertion.wire(), "removal": removal.wire()]
        if let animation { j["animation"] = animation._wire(token: 0) }
        return j
    }
}

public protocol Transition {
    associatedtype Body: View
    typealias Content = PlaceholderContentView<Self>
    @ViewBuilder func body(content: Self.Content, phase: TransitionPhase) -> Self.Body
    static var properties: TransitionProperties { get }
}

extension Transition {
    public static var properties: TransitionProperties { TransitionProperties() }
}

protocol _BuiltinTransition {
    func _transitionSpec() -> _TransitionSpec
}

/// Extract a `_TransitionEffect` from any custom `Transition`'s `body(content:phase:)` by
/// evaluating it on a dummy placeholder node and reading the opacity/offset modifiers it attached.
func _extractTransitionEffect<T: Transition>(_ t: T, phase: TransitionPhase) -> _TransitionEffect {
    let placeholder = PlaceholderContentView<T>(emitChildren: { p in [["kind": "empty", "id": p]] })
    let node = _eval(t.body(content: placeholder, phase: phase), "__tr")
    var eff = _TransitionEffect()
    if let mods = node["modifiers"] as? [Node] {
        for m in mods {
            if let op = m["opacity"] as? Double { eff.opacity = op }
            if let off = m["offset"] as? [String: Any] {
                eff.offsetX = (eff.offsetX ?? 0) + _f64(off["x"])
                eff.offsetY = (eff.offsetY ?? 0) + _f64(off["y"])
            }
        }
    }
    return eff
}

func _specForTransition<T: Transition>(_ t: T) -> _TransitionSpec {
    if let b = t as? _BuiltinTransition { return b._transitionSpec() }
    return _TransitionSpec(
        insertion: _extractTransitionEffect(t, phase: .willAppear),
        removal: _extractTransitionEffect(t, phase: .didDisappear)
    )
}

public struct IdentityTransition: Transition, _BuiltinTransition, Sendable {
    public init() {}
    public static var properties: TransitionProperties { TransitionProperties(hasMotion: false) }
    public func body(content: Content, phase: TransitionPhase) -> some View { content }
    func _transitionSpec() -> _TransitionSpec { _TransitionSpec() }
}

public struct OpacityTransition: Transition, _BuiltinTransition, Sendable {
    public init() {}
    public static var properties: TransitionProperties { TransitionProperties(hasMotion: false) }
    public func body(content: Content, phase: TransitionPhase) -> some View {
        content.opacity(phase.isIdentity ? 1 : 0)
    }
    func _transitionSpec() -> _TransitionSpec {
        let e = _TransitionEffect(opacity: 0)
        return _TransitionSpec(insertion: e, removal: e)
    }
}

public struct ScaleTransition: Transition, _BuiltinTransition, Sendable {
    public var scale: Double
    public var anchor: UnitPoint
    public init(_ scale: Double, anchor: UnitPoint = .center) { self.scale = scale; self.anchor = anchor }
    public func body(content: Content, phase: TransitionPhase) -> some View { content }
    func _transitionSpec() -> _TransitionSpec {
        let e = _TransitionEffect(scale: scale, anchor: anchor)
        return _TransitionSpec(insertion: e, removal: e)
    }
}

public struct MoveTransition: Transition, _BuiltinTransition, Sendable {
    public var edge: Edge
    public init(edge: Edge) { self.edge = edge }
    public func body(content: Content, phase: TransitionPhase) -> some View { content }
    func _transitionSpec() -> _TransitionSpec {
        let e = _TransitionEffect(edge: edge)
        return _TransitionSpec(insertion: e, removal: e)
    }
}

public struct SlideTransition: Transition, _BuiltinTransition, Sendable {
    public init() {}
    public func body(content: Content, phase: TransitionPhase) -> some View { content }
    func _transitionSpec() -> _TransitionSpec {
        _TransitionSpec(insertion: _TransitionEffect(edge: .leading), removal: _TransitionEffect(edge: .trailing))
    }
}

public struct OffsetTransition: Transition, _BuiltinTransition, Sendable {
    public var offset: CGSize
    public init(_ offset: CGSize) { self.offset = offset }
    public func body(content: Content, phase: TransitionPhase) -> some View {
        content.offset(phase.isIdentity ? .zero : offset)
    }
    func _transitionSpec() -> _TransitionSpec {
        let e = _TransitionEffect(offsetX: Double(offset.width), offsetY: Double(offset.height))
        return _TransitionSpec(insertion: e, removal: e)
    }
}

public struct PushTransition: Transition, _BuiltinTransition, Sendable {
    public var edge: Edge
    public init(edge: Edge) { self.edge = edge }
    public func body(content: Content, phase: TransitionPhase) -> some View { content }
    func _transitionSpec() -> _TransitionSpec {
        _TransitionSpec(
            insertion: _TransitionEffect(opacity: 0, edge: edge),
            removal: _TransitionEffect(opacity: 0, edge: _oppositeEdge(edge))
        )
    }
}

public struct AsymmetricTransition<Insertion: Transition, Removal: Transition>: Transition, _BuiltinTransition {
    public var insertion: Insertion
    public var removal: Removal
    public init(insertion: Insertion, removal: Removal) { self.insertion = insertion; self.removal = removal }
    public func body(content: Content, phase: TransitionPhase) -> some View { content }
    func _transitionSpec() -> _TransitionSpec {
        _TransitionSpec(
            insertion: _specForTransition(insertion).insertion,
            removal: _specForTransition(removal).removal
        )
    }
}

public struct BlurReplaceTransition: Transition, _BuiltinTransition, Sendable {
    public struct Configuration: Equatable, Hashable, Sendable {
        fileprivate let removalScale: CGFloat
        public static let downUp = Configuration(removalScale: 0.9)
        public static let upUp = Configuration(removalScale: 1.1)
    }
    private static let insertionScale: CGFloat = 0.9
    private static let blurRadius = 10.0
    public var configuration: Configuration
    public init(configuration: Configuration = .downUp) { self.configuration = configuration }
    public func body(content: Content, phase: TransitionPhase) -> some View {
        let scale = phase == .didDisappear ? configuration.removalScale : Self.insertionScale
        return content
            .scaleEffect(phase.isIdentity ? 1 : scale)
            .blur(radius: phase.isIdentity ? 0 : CGFloat(Self.blurRadius))
            .opacity(phase.isIdentity ? 1 : 0)
    }
    func _transitionSpec() -> _TransitionSpec {
        _TransitionSpec(
            insertion: _TransitionEffect(opacity: 0, scale: Double(Self.insertionScale), blur: Self.blurRadius),
            removal: _TransitionEffect(opacity: 0, scale: Double(configuration.removalScale), blur: Self.blurRadius)
        )
    }
}

extension Transition where Self == IdentityTransition {
    public static var identity: IdentityTransition { IdentityTransition() }
}
extension Transition where Self == OpacityTransition {
    public static var opacity: OpacityTransition { OpacityTransition() }
}
extension Transition where Self == ScaleTransition {
    public static var scale: ScaleTransition { ScaleTransition(0) }
    public static func scale(_ scale: Double, anchor: UnitPoint = .center) -> ScaleTransition { ScaleTransition(scale, anchor: anchor) }
}
extension Transition where Self == SlideTransition {
    public static var slide: SlideTransition { SlideTransition() }
}
extension Transition where Self == MoveTransition {
    public static func move(edge: Edge) -> MoveTransition { MoveTransition(edge: edge) }
}
extension Transition where Self == OffsetTransition {
    public static func offset(_ offset: CGSize) -> OffsetTransition { OffsetTransition(offset) }
    public static func offset(x: CGFloat = 0, y: CGFloat = 0) -> OffsetTransition { OffsetTransition(CGSize(width: x, height: y)) }
}
extension Transition where Self == PushTransition {
    public static func push(from edge: Edge) -> PushTransition { PushTransition(edge: edge) }
}
extension Transition where Self == BlurReplaceTransition {
    public static var blurReplace: BlurReplaceTransition { BlurReplaceTransition() }
    public static func blurReplace(_ configuration: BlurReplaceTransition.Configuration) -> BlurReplaceTransition {
        BlurReplaceTransition(configuration: configuration)
    }
}

public struct AnyTransition {
    var spec: _TransitionSpec
    init(spec: _TransitionSpec) { self.spec = spec }

    public init<T: Transition>(_ transition: T) {
        self.spec = _specForTransition(transition)
    }

    public static let identity = AnyTransition(IdentityTransition())
    public static let opacity = AnyTransition(OpacityTransition())
    public static let slide = AnyTransition(SlideTransition())
    public static let scale = AnyTransition(ScaleTransition(0))
    public static func scale(scale: CGFloat, anchor: UnitPoint = .center) -> AnyTransition {
        AnyTransition(ScaleTransition(Double(scale), anchor: anchor))
    }
    public static func move(edge: Edge) -> AnyTransition { AnyTransition(MoveTransition(edge: edge)) }
    public static func offset(_ offset: CGSize) -> AnyTransition { AnyTransition(OffsetTransition(offset)) }
    public static func offset(x: CGFloat = 0, y: CGFloat = 0) -> AnyTransition {
        AnyTransition(OffsetTransition(CGSize(width: x, height: y)))
    }
    public static func push(from edge: Edge) -> AnyTransition { AnyTransition(PushTransition(edge: edge)) }
    public static var blurReplace: AnyTransition { AnyTransition(BlurReplaceTransition()) }
    public static func asymmetric(insertion: AnyTransition, removal: AnyTransition) -> AnyTransition {
        AnyTransition(spec: _TransitionSpec(insertion: insertion.spec.insertion, removal: removal.spec.removal))
    }

    public func combined(with other: AnyTransition) -> AnyTransition {
        AnyTransition(spec: _TransitionSpec(
            insertion: spec.insertion.merged(with: other.spec.insertion),
            removal: spec.removal.merged(with: other.spec.removal),
            animation: other.spec.animation ?? spec.animation
        ))
    }

    public func animation(_ animation: Animation?) -> AnyTransition {
        var copy = self
        copy.spec.animation = animation
        return copy
    }
}

extension View {
    public func transition(_ t: AnyTransition) -> some View {
        _modified { ["transition": t.spec.wire()] }
    }

    public func transition<T: Transition>(_ transition: T) -> some View {
        _modified { ["transition": _specForTransition(transition).wire()] }
    }
}

// MARK: - @Namespace & .matchedGeometryEffect

@propertyWrapper
public struct Namespace: _StateProperty {
    public struct ID: Hashable, Sendable {
        let raw: String
    }

    final class Storage {
        var slot: _Slot<String>?
    }
    let storage = Storage()

    public init() {}

    func _bind(_ key: String) {
        let rt = _Runtime.shared
        rt.touched.insert(key)
        if let s = rt.slots[key] as? _Slot<String> { storage.slot = s; return }
        let s = _Slot(key)
        rt.slots[key] = s
        storage.slot = s
    }

    public var wrappedValue: Namespace.ID {
        ID(raw: storage.slot?.value ?? "ns#unbound")
    }
}

public struct MatchedGeometryProperties: OptionSet, Sendable {
    public let rawValue: UInt32
    public init(rawValue: UInt32) { self.rawValue = rawValue }
    public static let position = MatchedGeometryProperties(rawValue: 1)
    public static let size = MatchedGeometryProperties(rawValue: 2)
    public static let frame: MatchedGeometryProperties = [.position, .size]

    var wireName: String {
        if contains(.frame) { return "frame" }
        if contains(.size) { return "size" }
        return "position"
    }
}

extension View {
    public func matchedGeometryEffect<ID: Hashable>(
        id: ID,
        in namespace: Namespace.ID,
        properties: MatchedGeometryProperties = .frame,
        anchor: UnitPoint = .center,
        isSource: Bool = true
    ) -> some View {
        _modified {
            [
                "matchedGeometry": [
                    "id": _idKey(id),
                    "namespace": namespace.raw,
                    "properties": properties.wireName,
                    "anchor": [Double(anchor.x), Double(anchor.y)],
                    "isSource": isSource,
                ] as Node
            ]
        }
    }
}

// MARK: - ContentTransition

public struct ContentTransition: Equatable, Sendable {
    enum Kind: Equatable {
        case identity
        case interpolate
        case opacity
        case numericText(countsDown: Bool)
        case symbolEffect
    }
    let kind: Kind

    public static let identity = ContentTransition(kind: .identity)
    public static let interpolate = ContentTransition(kind: .interpolate)
    public static let opacity = ContentTransition(kind: .opacity)
    public static func numericText(countsDown: Bool = false) -> ContentTransition {
        ContentTransition(kind: .numericText(countsDown: countsDown))
    }
    public static func numericText(value: Double) -> ContentTransition {
        let _ = value
        return ContentTransition(kind: .numericText(countsDown: false))
    }
    public static var symbolEffect: ContentTransition { ContentTransition(kind: .symbolEffect) }

    var wireName: String {
        switch kind {
        case .identity: return "identity"
        case .interpolate: return "interpolate"
        case .opacity: return "opacity"
        case .numericText(let down): return down ? "numericTextDown" : "numericTextUp"
        case .symbolEffect: return "symbolEffect"
        }
    }
}

struct _ContentTransitionView<Content: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content, transition: ContentTransition
    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared
        let saved = rt.contentTransition
        rt.contentTransition = transition
        defer { rt.contentTransition = saved }
        return _modTargets(content, path).map {
            var n = $0
            n["modifiers"] = ((n["modifiers"] as? [Node]) ?? []) + [["contentTransition": transition.wireName]]
            return n
        }
    }
}

extension View {
    public func contentTransition(_ transition: ContentTransition) -> some View {
        _ContentTransitionView(content: self, transition: transition)
    }
}

// MARK: - PhaseAnimator

final class _PhaseState {
    var index: Int = 0
    var running: Bool = false
    var initialized: Bool = false
    var lastTrigger: Any? = nil
    var stepStartMs: Double = 0
    var stepDurationMs: Double = 0
    var pendingAnim: Animation? = nil
}

public struct PhaseAnimator<Phase: Equatable, Content: View>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }

    let phases: [Phase]
    let trigger: Any?
    let triggerChanged: ((Any?) -> Bool)?
    let content: (Phase) -> Content
    let animation: (Phase) -> Animation?

    public init<S: Sequence>(
        _ phases: S,
        @ViewBuilder content: @escaping (Phase) -> Content,
        animation: @escaping (Phase) -> Animation? = { _ in .default }
    ) where S.Element == Phase {
        self.phases = Array(phases)
        self.trigger = nil
        self.triggerChanged = nil
        self.content = content
        self.animation = animation
    }

    public init<S: Sequence, T: Equatable>(
        _ phases: S,
        trigger: T,
        @ViewBuilder content: @escaping (Phase) -> Content,
        animation: @escaping (Phase) -> Animation? = { _ in .default }
    ) where S.Element == Phase {
        self.phases = Array(phases)
        self.trigger = trigger
        self.triggerChanged = { prev in (prev as? T) != trigger }
        self.content = content
        self.animation = animation
    }

    func _children(_ path: String) -> [Node] {
        guard !phases.isEmpty else { return [] }
        let rt = _Runtime.shared
        let key = path + "#phase"
        rt.touched.insert(key)
        let st: _PhaseState
        if let existing = rt.slots[key] as? _PhaseState {
            st = existing
        } else {
            st = _PhaseState()
            rt.slots[key] = st
        }
        let continuous = triggerChanged == nil
        if !st.initialized {
            st.initialized = true
            st.lastTrigger = trigger
            st.index = 0
            if continuous && phases.count > 1 {
                st.running = true
                st.stepStartMs = rt.clockMs
                // Start animating phase 0 -> phase 1 immediately on the first tick (matching iOS).
                st.stepDurationMs = 0
            }
        } else if let changed = triggerChanged, changed(st.lastTrigger) {
            st.lastTrigger = trigger
            if phases.count > 1 {
                st.index = 1
                st.running = true
                st.stepStartMs = rt.clockMs
                let stepAnim = animation(phases[1])
                st.pendingAnim = stepAnim
                st.stepDurationMs = max((stepAnim?._totalDuration ?? 0.35) * 1000.0, 16.0)
            }
        }

        let currentIndex = min(st.index, phases.count - 1)
        let phase = phases[currentIndex]
        let animForPass = st.pendingAnim
        st.pendingAnim = nil

        if st.running && phases.count > 1 {
            let due = st.stepStartMs + st.stepDurationMs
            rt.requestSchedule(max(1.0, due - rt.clockMs))
            let phasesCopy = phases
            let animFn = animation
            rt.tickHandlers.append {
                guard st.running else { return }
                let now = rt.clockMs
                if now + 0.5 >= st.stepStartMs + st.stepDurationMs {
                    let next = (st.index + 1) % phasesCopy.count
                    st.index = next
                    let stepAnim = animFn(phasesCopy[next])
                    st.pendingAnim = stepAnim
                    st.stepStartMs = now
                    st.stepDurationMs = max((stepAnim?._totalDuration ?? 0.35) * 1000.0, 16.0)
                    if !continuous && next == 0 {
                        st.running = false
                    }
                    rt.dirty = true
                } else {
                    rt.requestSchedule(max(1.0, (st.stepStartMs + st.stepDurationMs) - now))
                }
            }
        }

        let built = SwiftUI._children(content(phase), path)
        if let animForPass {
            let mod: Node = ["animation": animForPass._wire(token: rt.nextAnimToken())]
            return built.map { var n = $0; n["modifiers"] = ((n["modifiers"] as? [Node]) ?? []) + [mod]; return n }
        }
        return built
    }
}

extension View {
    public func phaseAnimator<Phase: Equatable, Content: View>(
        _ phases: some Sequence<Phase>,
        @ViewBuilder content: @escaping (PlaceholderContentView<Self>, Phase) -> Content,
        animation: @escaping (Phase) -> Animation? = { _ in .default }
    ) -> some View {
        let placeholder = PlaceholderContentView<Self>(content: self)
        return PhaseAnimator(phases, content: { phase in content(placeholder, phase) }, animation: animation)
    }

    public func phaseAnimator<Phase: Equatable, Content: View>(
        _ phases: some Sequence<Phase>,
        trigger: some Equatable,
        @ViewBuilder content: @escaping (PlaceholderContentView<Self>, Phase) -> Content,
        animation: @escaping (Phase) -> Animation? = { _ in .default }
    ) -> some View {
        let placeholder = PlaceholderContentView<Self>(content: self)
        return PhaseAnimator(phases, trigger: trigger, content: { phase in content(placeholder, phase) }, animation: animation)
    }
}

// MARK: - KeyframeAnimator & KeyframeTimeline

struct _KeyframeSegment<Value: Animatable> {
    enum Kind {
        case linear(UnitCurve)
        case cubic
        case spring(Spring, Value?)
        case move
    }
    let to: Value
    let duration: TimeInterval
    let kind: Kind
}

public protocol KeyframeTrackContent<Value> {
    associatedtype Value: Animatable
    associatedtype Body: KeyframeTrackContent where Body.Value == Value
    @KeyframeTrackContentBuilder<Value> var body: Body { get }
}

extension KeyframeTrackContent where Body == Self {
    public var body: Self { fatalError("\(Self.self) is a primitive keyframe") }
}

public struct LinearKeyframe<Value: Animatable>: KeyframeTrackContent {
    public typealias Body = Self
    public let to: Value
    public let duration: TimeInterval
    public let timingCurve: UnitCurve

    public init(_ to: Value, duration: TimeInterval, timingCurve: UnitCurve = .linear) {
        self.to = to; self.duration = max(duration, 0); self.timingCurve = timingCurve
    }
    func _typedSegments() -> [_KeyframeSegment<Value>] {
        [_KeyframeSegment(to: to, duration: duration, kind: .linear(timingCurve))]
    }
}

public struct CubicKeyframe<Value: Animatable>: KeyframeTrackContent {
    public typealias Body = Self
    public let to: Value
    public let duration: TimeInterval

    public init(_ to: Value, duration: TimeInterval) {
        self.to = to; self.duration = max(duration, 0)
    }
    func _typedSegments() -> [_KeyframeSegment<Value>] {
        [_KeyframeSegment(to: to, duration: duration, kind: .cubic)]
    }
}

public struct SpringKeyframe<Value: Animatable>: KeyframeTrackContent {
    public typealias Body = Self
    public let to: Value
    public let duration: TimeInterval
    public let spring: Spring
    public let startVelocity: Value?

    public init(_ to: Value, duration: TimeInterval? = nil, spring: Spring = Spring(), startVelocity: Value? = nil) {
        self.to = to; self.spring = spring; self.duration = max(duration ?? spring.settlingDuration, 0); self.startVelocity = startVelocity
    }
    func _typedSegments() -> [_KeyframeSegment<Value>] {
        [_KeyframeSegment(to: to, duration: duration, kind: .spring(spring, startVelocity))]
    }
}

public struct MoveKeyframe<Value: Animatable>: KeyframeTrackContent {
    public typealias Body = Self
    public let to: Value

    public init(_ to: Value) { self.to = to }
    func _typedSegments() -> [_KeyframeSegment<Value>] {
        [_KeyframeSegment(to: to, duration: 0, kind: .move)]
    }
}

public struct _KeyframeList<Value: Animatable>: KeyframeTrackContent {
    public typealias Body = Self
    let items: [_KeyframeSegment<Value>]
    func _typedSegments() -> [_KeyframeSegment<Value>] { items }
}

func _segmentsOf<C: KeyframeTrackContent>(_ c: C) -> [_KeyframeSegment<C.Value>] {
    if let l = c as? _KeyframeList<C.Value> { return l.items }
    if let l = c as? LinearKeyframe<C.Value> { return l._typedSegments() }
    if let l = c as? CubicKeyframe<C.Value> { return l._typedSegments() }
    if let l = c as? SpringKeyframe<C.Value> { return l._typedSegments() }
    if let l = c as? MoveKeyframe<C.Value> { return l._typedSegments() }
    return _segmentsOf(c.body)
}

@resultBuilder
public struct KeyframeTrackContentBuilder<Value: Animatable> {
    public static func buildExpression<K: KeyframeTrackContent>(_ expression: K) -> _KeyframeList<Value> where K.Value == Value {
        _KeyframeList(items: _segmentsOf(expression))
    }
    public static func buildBlock(_ components: _KeyframeList<Value>...) -> _KeyframeList<Value> {
        _KeyframeList(items: components.flatMap { $0.items })
    }
    public static func buildOptional(_ component: _KeyframeList<Value>?) -> _KeyframeList<Value> {
        component ?? _KeyframeList(items: [])
    }
    public static func buildEither(first component: _KeyframeList<Value>) -> _KeyframeList<Value> { component }
    public static func buildEither(second component: _KeyframeList<Value>) -> _KeyframeList<Value> { component }
    public static func buildArray(_ components: [_KeyframeList<Value>]) -> _KeyframeList<Value> {
        _KeyframeList(items: components.flatMap { $0.items })
    }
}

/// Evaluates one track of keyframes on `Value` at elapsed `time` seconds.
func _evalTrack<Value: Animatable>(initial: Value, segments: [_KeyframeSegment<Value>], time: TimeInterval) -> Value {
    guard !segments.isEmpty else { return initial }
    if time <= 0 {
        if case .move = segments[0].kind { return segments[0].to }
        return initial
    }
    // Build start values for every segment (for Catmull-Rom tangents on CubicKeyframe).
    var starts: [Value] = [initial]
    for seg in segments { starts.append(seg.to) }

    var cursor = 0.0
    var cur = initial
    for (idx, seg) in segments.enumerated() {
        if case .move = seg.kind {
            cur = seg.to
            continue
        }
        let end = cursor + seg.duration
        if time < end || idx == segments.count - 1 {
            let lt = max(0, min(time - cursor, seg.duration))
            if seg.duration <= 1e-9 || time >= end {
                cur = seg.to
                if time < end { return cur }
                cursor = end
                continue
            }
            let u = lt / seg.duration
            var out = cur
            switch seg.kind {
            case .linear(let curve):
                let p = curve.value(at: u)
                out.animatableData = cur.animatableData.interpolated(towards: seg.to.animatableData, amount: p)
            case .cubic:
                // Catmull-Rom Hermite interpolation across neighboring keyframe values.
                let p0 = starts[max(idx - 1, 0)].animatableData
                let p1 = cur.animatableData
                let p2 = seg.to.animatableData
                let p3 = starts[min(idx + 2, starts.count - 1)].animatableData
                let m1 = idx == 0 ? (p2 - p1) : (p2 - p0).scaled(by: 0.5)
                let m2 = idx + 1 >= segments.count ? (p2 - p1) : (p3 - p1).scaled(by: 0.5)
                let u2 = u * u, u3 = u2 * u
                let h00 = 2 * u3 - 3 * u2 + 1
                let h10 = u3 - 2 * u2 + u
                let h01 = -2 * u3 + 3 * u2
                let h11 = u3 - u2
                out.animatableData = p1.scaled(by: h00) + m1.scaled(by: h10) + p2.scaled(by: h01) + m2.scaled(by: h11)
            case .spring(let sp, let vel):
                let delta = seg.to.animatableData - cur.animatableData
                let v0 = vel?.animatableData ?? .zero
                out.animatableData = cur.animatableData + sp.value(target: delta, initialVelocity: v0, time: lt)
            case .move:
                out = seg.to
            }
            return out
        }
        cur = seg.to
        cursor = end
    }
    return cur
}

struct _ErasedTrack<Root> {
    let duration: TimeInterval
    let apply: (inout Root, TimeInterval) -> Void
}

public protocol Keyframes<Value> {
    associatedtype Value
    associatedtype Body: Keyframes where Body.Value == Value
    @KeyframesBuilder<Value> var body: Body { get }
}

extension Keyframes where Body == Self {
    public var body: Self { fatalError("\(Self.self) is a primitive keyframe container") }
}

protocol _TrackProvider {
    associatedtype Root
    func _erasedTracks() -> [_ErasedTrack<Root>]
}

public struct KeyframeTrack<Root, Value: Animatable, Content: KeyframeTrackContent>: Keyframes, _TrackProvider where Content.Value == Value {
    public typealias Value = Root
    public typealias Body = Self
    let keyPath: WritableKeyPath<Root, Value>
    let segments: [_KeyframeSegment<Value>]

    public init(_ keyPath: WritableKeyPath<Root, Value>, @KeyframeTrackContentBuilder<Value> content: () -> Content) {
        self.keyPath = keyPath
        self.segments = _segmentsOf(content())
    }

    public init(@KeyframeTrackContentBuilder<Value> content: () -> Content) where Root == Value {
        self.keyPath = \Root.self
        self.segments = _segmentsOf(content())
    }

    func _erasedTracks() -> [_ErasedTrack<Root>] {
        let kp = keyPath
        let segs = segments
        let dur = segs.reduce(0.0) { $0 + $1.duration }
        return [_ErasedTrack(duration: dur) { root, t in
            root[keyPath: kp] = _evalTrack(initial: root[keyPath: kp], segments: segs, time: t)
        }]
    }
}

public struct _KeyframesList<Value>: Keyframes, _TrackProvider {
    public typealias Body = Self
    let tracks: [_ErasedTrack<Value>]
    func _erasedTracks() -> [_ErasedTrack<Value>] { tracks }
}

func _tracksOf<K: Keyframes>(_ k: K) -> [_ErasedTrack<K.Value>] {
    if let l = k as? _KeyframesList<K.Value> { return l.tracks }
    if let p = k as? any _TrackProvider {
        // Use a local generic trampoline via a typed helper when possible; fallback to body.
        if let tracks = _castTracks(p, as: K.Value.self) { return tracks }
    }
    return _tracksOf(k.body)
}

private func _castTracks<P: _TrackProvider, V>(_ p: P, as _: V.Type) -> [_ErasedTrack<V>]? {
    p._erasedTracks() as? [_ErasedTrack<V>]
}

@resultBuilder
public struct KeyframesBuilder<Value> {
    public static func buildExpression<K: Keyframes>(_ expression: K) -> _KeyframesList<Value> where K.Value == Value {
        _KeyframesList(tracks: _tracksOf(expression))
    }
    public static func buildBlock(_ components: _KeyframesList<Value>...) -> _KeyframesList<Value> {
        _KeyframesList(tracks: components.flatMap { $0.tracks })
    }
    public static func buildOptional(_ component: _KeyframesList<Value>?) -> _KeyframesList<Value> {
        component ?? _KeyframesList(tracks: [])
    }
    public static func buildEither(first component: _KeyframesList<Value>) -> _KeyframesList<Value> { component }
    public static func buildEither(second component: _KeyframesList<Value>) -> _KeyframesList<Value> { component }
}

public struct KeyframeTimeline<Value> {
    let initialValue: Value
    let tracks: [_ErasedTrack<Value>]
    public let duration: TimeInterval

    public init<K: Keyframes>(initialValue: Value, @KeyframesBuilder<Value> content: () -> K) where K.Value == Value {
        self.initialValue = initialValue
        self.tracks = _tracksOf(content())
        self.duration = self.tracks.reduce(0.0) { max($0, $1.duration) }
    }

    public func value(time: TimeInterval) -> Value {
        var out = initialValue
        for tr in tracks { tr.apply(&out, time) }
        return out
    }

    public func value(progress: Double) -> Value {
        value(time: progress * duration)
    }
}

final class _KeyframeState {
    var initialized = false
    var active = false
    var startMs: Double = 0
    var lastTrigger: Any? = nil
    var settledValue: Any? = nil
}

public struct KeyframeAnimator<Value, KeyframePath: Keyframes, Content: View>: View, _Flattenable where KeyframePath.Value == Value {
    public typealias Body = Never
    public var body: Never { fatalError() }

    let initialValue: Value
    let repeating: Bool
    let trigger: Any?
    let triggerChanged: ((Any?) -> Bool)?
    let content: (Value) -> Content
    let keyframes: (Value) -> KeyframePath

    public init(
        initialValue: Value,
        repeating: Bool = true,
        @ViewBuilder content: @escaping (Value) -> Content,
        @KeyframesBuilder<Value> keyframes: @escaping (Value) -> KeyframePath
    ) {
        self.initialValue = initialValue
        self.repeating = repeating
        self.trigger = nil
        self.triggerChanged = nil
        self.content = content
        self.keyframes = keyframes
    }

    public init<T: Equatable>(
        initialValue: Value,
        trigger: T,
        @ViewBuilder content: @escaping (Value) -> Content,
        @KeyframesBuilder<Value> keyframes: @escaping (Value) -> KeyframePath
    ) {
        self.initialValue = initialValue
        self.repeating = false
        self.trigger = trigger
        self.triggerChanged = { prev in (prev as? T) != trigger }
        self.content = content
        self.keyframes = keyframes
    }

    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared
        let key = path + "#keyframe"
        rt.touched.insert(key)
        let st: _KeyframeState
        if let existing = rt.slots[key] as? _KeyframeState {
            st = existing
        } else {
            st = _KeyframeState()
            rt.slots[key] = st
        }
        let timeline = KeyframeTimeline(initialValue: initialValue) { keyframes(initialValue) }

        if !st.initialized {
            st.initialized = true
            st.lastTrigger = trigger
            // Non-triggered KeyframeAnimator starts immediately (repeating or one-shot when repeating == false).
            if triggerChanged == nil && timeline.duration > 0 {
                st.active = true
                st.startMs = rt.clockMs
            }
        } else if let changed = triggerChanged, changed(st.lastTrigger) {
            st.lastTrigger = trigger
            if timeline.duration > 0 {
                st.active = true
                st.startMs = rt.clockMs
                st.settledValue = nil
            }
        }

        let currentVal: Value
        if st.active && timeline.duration > 0 {
            var elapsed = max(0, (rt.clockMs - st.startMs) / 1000.0)
            if repeating {
                elapsed = elapsed.truncatingRemainder(dividingBy: timeline.duration)
            } else if elapsed >= timeline.duration - 1e-6 {
                elapsed = timeline.duration
                st.active = false
                let finalVal = timeline.value(time: elapsed)
                st.settledValue = finalVal
                currentVal = finalVal
                return SwiftUI._children(content(currentVal), path)
            }
            currentVal = timeline.value(time: elapsed)
        } else if let settled = st.settledValue as? Value {
            currentVal = settled
        } else {
            currentVal = initialValue
        }

        if st.active {
            rt.requestSchedule(16.0)
            let dur = timeline.duration
            let rep = repeating
            rt.tickHandlers.append {
                guard st.active else { return }
                let elapsed = (rt.clockMs - st.startMs) / 1000.0
                if !rep && elapsed >= dur - 1e-6 {
                    // One final pass to settle at the end keyframe.
                    rt.dirty = true
                } else {
                    rt.dirty = true
                }
            }
        }

        return SwiftUI._children(content(currentVal), path)
    }
}

extension View {
    public func keyframeAnimator<Value, K: Keyframes, Content: View>(
        initialValue: Value,
        repeating: Bool = true,
        @ViewBuilder content: @escaping (PlaceholderContentView<Self>, Value) -> Content,
        @KeyframesBuilder<Value> keyframes: @escaping (Value) -> K
    ) -> some View where K.Value == Value {
        let placeholder = PlaceholderContentView<Self>(content: self)
        return KeyframeAnimator(initialValue: initialValue, repeating: repeating, content: { val in content(placeholder, val) }, keyframes: keyframes)
    }

    public func keyframeAnimator<Value, K: Keyframes, Content: View>(
        initialValue: Value,
        trigger: some Equatable,
        @ViewBuilder content: @escaping (PlaceholderContentView<Self>, Value) -> Content,
        @KeyframesBuilder<Value> keyframes: @escaping (Value) -> K
    ) -> some View where K.Value == Value {
        let placeholder = PlaceholderContentView<Self>(content: self)
        return KeyframeAnimator(initialValue: initialValue, trigger: trigger, content: { val in content(placeholder, val) }, keyframes: keyframes)
    }
}

// MARK: - TimelineView & TimelineSchedule

public enum TimelineScheduleMode: Equatable, Hashable, Sendable {
    case normal
    case lowFrequency
}

public protocol TimelineSchedule {
    associatedtype Entries: Sequence where Self.Entries.Element == Date
    func entries(from startDate: Date, mode: TimelineScheduleMode) -> Self.Entries
}

public struct PeriodicTimelineSchedule: TimelineSchedule {
    public let startDate: Date
    public let interval: TimeInterval

    public init(from startDate: Date, by interval: TimeInterval) {
        self.startDate = startDate
        self.interval = max(interval, 0.001)
    }

    public struct Entries: Sequence, IteratorProtocol {
        var current: Date
        let interval: TimeInterval
        public mutating func next() -> Date? {
            defer { current = current.addingTimeInterval(interval) }
            return current
        }
    }

    public func entries(from startDate: Date, mode: TimelineScheduleMode) -> Entries {
        let step = mode == .lowFrequency ? max(interval, 1.0) : interval
        let diff = startDate.timeIntervalSince(self.startDate)
        let first = diff <= 0 ? self.startDate : self.startDate.addingTimeInterval(floor(diff / step) * step)
        return Entries(current: first, interval: step)
    }
}

public struct AnimationTimelineSchedule: TimelineSchedule {
    public let minimumInterval: Double?
    public let paused: Bool

    public init(minimumInterval: Double? = nil, paused: Bool = false) {
        self.minimumInterval = minimumInterval
        self.paused = paused
    }

    public struct Entries: Sequence, IteratorProtocol {
        var current: Date
        let step: Double
        var emitted = false
        let paused: Bool
        public mutating func next() -> Date? {
            if paused {
                guard !emitted else { return nil }
                emitted = true
                return current
            }
            defer { current = current.addingTimeInterval(step) }
            return current
        }
    }

    public func entries(from startDate: Date, mode: TimelineScheduleMode) -> Entries {
        let step = max(minimumInterval ?? (mode == .lowFrequency ? 1.0 : (1.0 / 60.0)), 1.0 / 120.0)
        return Entries(current: startDate, step: step, paused: paused)
    }
}

public struct EveryMinuteTimelineSchedule: TimelineSchedule {
    public init() {}
    public struct Entries: Sequence, IteratorProtocol {
        var current: Date
        public mutating func next() -> Date? {
            defer { current = current.addingTimeInterval(60.0) }
            return current
        }
    }
    public func entries(from startDate: Date, mode: TimelineScheduleMode) -> Entries {
        let t = floor(startDate.timeIntervalSince1970 / 60.0) * 60.0
        return Entries(current: Date(timeIntervalSince1970: t))
    }
}

public struct ExplicitTimelineSchedule<Entries: Sequence>: TimelineSchedule where Entries.Element == Date {
    let allDates: [Date]
    public init(_ timeOffsets: Entries) {
        self.allDates = Array(timeOffsets).sorted()
    }
    public func entries(from startDate: Date, mode: TimelineScheduleMode) -> [Date] {
        guard !allDates.isEmpty else { return [] }
        // Keep the latest entry <= startDate plus all entries > startDate.
        var lastBefore: Date? = nil
        var future: [Date] = []
        for d in allDates {
            if d <= startDate { lastBefore = d } else { future.append(d) }
        }
        return (lastBefore.map { [$0] } ?? []) + future
    }
}

extension TimelineSchedule where Self == PeriodicTimelineSchedule {
    public static func periodic(from startDate: Date, by interval: TimeInterval) -> PeriodicTimelineSchedule {
        PeriodicTimelineSchedule(from: startDate, by: interval)
    }
}
extension TimelineSchedule where Self == AnimationTimelineSchedule {
    public static var animation: AnimationTimelineSchedule { AnimationTimelineSchedule() }
    public static func animation(minimumInterval: Double? = nil, paused: Bool = false) -> AnimationTimelineSchedule {
        AnimationTimelineSchedule(minimumInterval: minimumInterval, paused: paused)
    }
}
extension TimelineSchedule where Self == EveryMinuteTimelineSchedule {
    public static var everyMinute: EveryMinuteTimelineSchedule { EveryMinuteTimelineSchedule() }
}
extension TimelineSchedule {
    public static func explicit<S: Sequence>(_ timeOffsets: S) -> ExplicitTimelineSchedule<S> where Self == ExplicitTimelineSchedule<S>, S.Element == Date {
        ExplicitTimelineSchedule(timeOffsets)
    }
}

public struct TimelineViewDefaultContext {
    public enum Cadence: Equatable, Hashable, Comparable, Sendable {
        case live, seconds, minutes
        public static func < (lhs: Self, rhs: Self) -> Bool {
            let rank: (Self) -> Int = { switch $0 { case .live: return 0; case .seconds: return 1; case .minutes: return 2 } }
            return rank(lhs) < rank(rhs)
        }
    }
    public let date: Date
    public let cadence: Cadence
    public init(date: Date, cadence: Cadence = .live) { self.date = date; self.cadence = cadence }
}

public struct TimelineView<Schedule: TimelineSchedule, Content: View>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }

    public var schedule: Schedule
    public var content: (TimelineViewDefaultContext) -> Content

    public init(_ schedule: Schedule, @ViewBuilder content: @escaping (TimelineViewDefaultContext) -> Content) {
        self.schedule = schedule
        self.content = content
    }

    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared
        let now = rt.currentDate
        let cadence: TimelineViewDefaultContext.Cadence
        if schedule is AnimationTimelineSchedule { cadence = .live }
        else if schedule is EveryMinuteTimelineSchedule { cadence = .minutes }
        else { cadence = .seconds }

        var activeDate = now
        var nextDate: Date? = nil
        var iter = schedule.entries(from: now, mode: .normal).makeIterator()
        var steps = 0
        while let d = iter.next(), steps < 64 {
            steps += 1
            if d.timeIntervalSince1970 <= now.timeIntervalSince1970 + 1e-6 {
                activeDate = d
            } else {
                nextDate = d
                break
            }
        }

        if let next = nextDate {
            let delayMs = max(1.0, (next.timeIntervalSince1970 - now.timeIntervalSince1970) * 1000.0)
            rt.requestSchedule(delayMs)
            let nextEpoch = next.timeIntervalSince1970
            rt.tickHandlers.append {
                let cur = rt.currentDate.timeIntervalSince1970
                if cur + 1e-4 >= nextEpoch {
                    rt.dirty = true
                } else {
                    rt.requestSchedule(max(1.0, (nextEpoch - cur) * 1000.0))
                }
            }
        }

        let ctx = TimelineViewDefaultContext(date: activeDate, cadence: cadence)
        return SwiftUI._children(content(ctx), path)
    }
}
