package androidx.compose.animation.core

import androidx.compose.foundation.MutatePriority
import androidx.compose.foundation.MutatorMutex
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import kotlin.math.abs
import kotlin.math.exp
import kotlin.math.ln
import kotlin.math.roundToInt
import kotlin.math.sqrt

// ---- Easing ----------------------------------------------------------------------------------------------------------

/** Maps a fraction of an animation's duration (0...1) to the fraction of its value change. */
@Stable
fun interface Easing {
    fun transform(fraction: Float): Float
}

/** A cubic Bézier easing from (0, 0) through control points (a, b) and (c, d) to (1, 1). */
@Immutable
class CubicBezierEasing(private val a: Float, private val b: Float, private val c: Float, private val d: Float) : Easing {
    private fun bezier(p1: Float, p2: Float, m: Float) = 3 * p1 * (1 - m) * (1 - m) * m + 3 * p2 * (1 - m) * m * m + m * m * m

    override fun transform(fraction: Float): Float {
        if (fraction <= 0f || fraction >= 1f) return fraction.coerceIn(0f, 1f)
        var lo = 0f
        var hi = 1f
        repeat(32) {
            val mid = (lo + hi) / 2
            if (bezier(a, c, mid) < fraction) lo = mid else hi = mid
        }
        return bezier(b, d, (lo + hi) / 2)
    }

    override fun equals(other: Any?) = other is CubicBezierEasing && a == other.a && b == other.b && c == other.c && d == other.d
    override fun hashCode() = ((a.hashCode() * 31 + b.hashCode()) * 31 + c.hashCode()) * 31 + d.hashCode()
}

val FastOutSlowInEasing: Easing = CubicBezierEasing(0.4f, 0f, 0.2f, 1f)
val LinearOutSlowInEasing: Easing = CubicBezierEasing(0f, 0f, 0.2f, 1f)
val FastOutLinearInEasing: Easing = CubicBezierEasing(0.4f, 0f, 1f, 1f)
val LinearEasing: Easing = Easing { it }

// ---- Specs -----------------------------------------------------------------------------------------------------------

object AnimationConstants {
    const val DefaultDurationMillis: Int = 300
    const val UnspecifiedTime: Long = Long.MIN_VALUE
}

object Spring {
    const val StiffnessHigh = 10_000f
    const val StiffnessMedium = 1500f
    const val StiffnessMediumLow = 400f
    const val StiffnessLow = 200f
    const val StiffnessVeryLow = 50f
    const val DampingRatioHighBouncy = 0.2f
    const val DampingRatioMediumBouncy = 0.5f
    const val DampingRatioLowBouncy = 0.75f
    const val DampingRatioNoBouncy = 1f
    const val DefaultDisplacementThreshold = 0.01f
}

/** How a value animates to its target. */
interface AnimationSpec<T>

/** An [AnimationSpec] that ends. */
interface FiniteAnimationSpec<T> : AnimationSpec<T>

/** A duration-based spec: [durationMillis] after [delay], shaped by [easing]. */
@Immutable
class TweenSpec<T>(val durationMillis: Int = AnimationConstants.DefaultDurationMillis, val delay: Int = 0, val easing: Easing = FastOutSlowInEasing) :
    FiniteAnimationSpec<T> {
    override fun equals(other: Any?) = other is TweenSpec<*> && durationMillis == other.durationMillis && delay == other.delay && easing == other.easing
    override fun hashCode() = (durationMillis * 31 + delay) * 31 + easing.hashCode()
}

/** A physics spring: [stiffness] and [dampingRatio]; settles within [visibilityThreshold] of the target. */
@Immutable
class SpringSpec<T>(val dampingRatio: Float = Spring.DampingRatioNoBouncy, val stiffness: Float = Spring.StiffnessMedium, val visibilityThreshold: T? = null) :
    FiniteAnimationSpec<T> {
    override fun equals(other: Any?) = other is SpringSpec<*> && dampingRatio == other.dampingRatio && stiffness == other.stiffness &&
        visibilityThreshold == other.visibilityThreshold
    override fun hashCode() = (dampingRatio.hashCode() * 31 + stiffness.hashCode()) * 31 + visibilityThreshold.hashCode()
}

/** Jumps to the target after [delay] ms. */
@Immutable
class SnapSpec<T>(val delay: Int = 0) : FiniteAnimationSpec<T> {
    override fun equals(other: Any?) = other is SnapSpec<*> && delay == other.delay
    override fun hashCode() = delay
}

@Stable
fun <T> tween(durationMillis: Int = AnimationConstants.DefaultDurationMillis, delayMillis: Int = 0, easing: Easing = FastOutSlowInEasing): TweenSpec<T> =
    TweenSpec(durationMillis, delayMillis, easing)

@Stable
fun <T> spring(dampingRatio: Float = Spring.DampingRatioNoBouncy, stiffness: Float = Spring.StiffnessMedium, visibilityThreshold: T? = null): SpringSpec<T> =
    SpringSpec(dampingRatio, stiffness, visibilityThreshold)

@Stable
fun <T> snap(delayMillis: Int = 0): SnapSpec<T> = SnapSpec(delayMillis)

// ---- Decay -----------------------------------------------------------------------------------------------------------

/** Decelerates a value from an initial velocity until it stops. Values are floats (pixels for flings). */
interface DecayAnimationSpec<T> {
    /** Value change after [playTimeNanos] from [initialVelocity] (units per second). */
    fun valueChange(playTimeNanos: Long, initialVelocity: Float): Float
    fun velocityAt(playTimeNanos: Long, initialVelocity: Float): Float
    fun durationNanos(initialVelocity: Float): Long
    /** Where a decay from [initialValue] with [initialVelocity] comes to rest. */
    fun targetValue(initialValue: Float, initialVelocity: Float): Float =
        initialValue + valueChange(durationNanos(initialVelocity), initialVelocity)
}

fun <T> DecayAnimationSpec<T>.calculateTargetValue(initialValue: Float, initialVelocity: Float): Float =
    targetValue(initialValue, initialVelocity)

/** Exponential friction: velocity `v0 * e^(-t * 4.2 * friction)` until under [absVelocityThreshold]. */
fun <T> exponentialDecay(frictionMultiplier: Float = 1f, absVelocityThreshold: Float = 0.1f): DecayAnimationSpec<T> =
    ExponentialDecay(frictionMultiplier, absVelocityThreshold)

private class ExponentialDecay<T>(frictionMultiplier: Float, private val threshold: Float) : DecayAnimationSpec<T> {
    private val friction = -4.2f * frictionMultiplier.coerceAtLeast(0.0001f)

    override fun valueChange(playTimeNanos: Long, initialVelocity: Float): Float {
        val t = playTimeNanos / 1_000_000L
        return initialVelocity / friction * (exp(friction * t / 1000f) - 1f)
    }

    override fun velocityAt(playTimeNanos: Long, initialVelocity: Float): Float =
        initialVelocity * exp(playTimeNanos / 1_000_000L / 1000f * friction)

    override fun durationNanos(initialVelocity: Float): Long {
        if (abs(initialVelocity) <= threshold) return 0L
        return (1000f * ln(threshold / abs(initialVelocity)) / friction).toLong() * 1_000_000L
    }
}

// ---- Value conversion ------------------------------------------------------------------------------------------------

/** Converts an animated type to and from its float components. */
interface TwoWayConverter<T> {
    val convertToVector: (T) -> FloatArray
    val convertFromVector: (FloatArray) -> T
}

private class Converter<T>(override val convertToVector: (T) -> FloatArray, override val convertFromVector: (FloatArray) -> T) : TwoWayConverter<T>

fun <T> TwoWayConverter(convertToVector: (T) -> FloatArray, convertFromVector: (FloatArray) -> T): TwoWayConverter<T> =
    Converter(convertToVector, convertFromVector)

val Float.Companion.VectorConverter: TwoWayConverter<Float> get() = FloatConverter
val Dp.Companion.VectorConverter: TwoWayConverter<Dp> get() = DpConverter
val Offset.Companion.VectorConverter: TwoWayConverter<Offset> get() = OffsetConverter
val IntOffset.Companion.VectorConverter: TwoWayConverter<IntOffset> get() = IntOffsetConverter
val Int.Companion.VectorConverter: TwoWayConverter<Int> get() = IntConverter

internal val FloatConverter = TwoWayConverter<Float>({ floatArrayOf(it) }, { it[0] })
internal val IntConverter = TwoWayConverter<Int>({ floatArrayOf(it.toFloat()) }, { it[0].roundToInt() })
internal val DpConverter = TwoWayConverter<Dp>({ floatArrayOf(it.value) }, { Dp(it[0]) })
internal val OffsetConverter = TwoWayConverter<Offset>({ floatArrayOf(it.x, it.y) }, { Offset(it[0], it[1]) })
internal val IntOffsetConverter = TwoWayConverter<IntOffset>({ floatArrayOf(it.x.toFloat(), it.y.toFloat()) }, { IntOffset(it[0].roundToInt(), it[1].roundToInt()) })
internal val ColorConverter = TwoWayConverter<Color>({ floatArrayOf(it.red, it.green, it.blue, it.alpha) }, {
    Color(it[0].coerceIn(0f, 1f), it[1].coerceIn(0f, 1f), it[2].coerceIn(0f, 1f), it[3].coerceIn(0f, 1f))
})

// ---- Spring physics --------------------------------------------------------------------------------------------------

/** Position and velocity of a damped spring released at [x0] (relative to the target) with velocity [v0], after [t] s. */
internal fun springAt(x0: Float, v0: Float, t: Float, stiffness: Float, damping: Float): Pair<Float, Float> {
    val w = sqrt(stiffness.toDouble())
    val z = damping.toDouble()
    val td = t.toDouble()
    val x = x0.toDouble()
    val v = v0.toDouble()
    return when {
        z > 1.0 -> {
            val s = w * sqrt(z * z - 1)
            val gp = -z * w + s
            val gm = -z * w - s
            val b = (gm * x - v) / (gm - gp)
            val a = x - b
            val pos = a * exp(gm * td) + b * exp(gp * td)
            val vel = a * gm * exp(gm * td) + b * gp * exp(gp * td)
            pos.toFloat() to vel.toFloat()
        }
        z == 1.0 -> {
            val a = x
            val b = v + w * x
            val e = exp(-w * td)
            val pos = (a + b * td) * e
            val vel = (b - w * (a + b * td)) * e
            pos.toFloat() to vel.toFloat()
        }
        else -> {
            val wd = w * sqrt(1 - z * z)
            val a = x
            val b = (z * w * x + v) / wd
            val e = exp(-z * w * td)
            val c = kotlin.math.cos(wd * td)
            val sn = kotlin.math.sin(wd * td)
            val pos = e * (a * c + b * sn)
            val vel = e * ((-z * w) * (a * c + b * sn) + (-a * wd * sn + b * wd * c))
            pos.toFloat() to vel.toFloat()
        }
    }
}

// ---- Animatable ------------------------------------------------------------------------------------------------------

enum class AnimationEndReason { BoundReached, Finished }

class AnimationResult<T>(val endState: T, val endReason: AnimationEndReason) {
    override fun toString() = "AnimationResult(endReason=$endReason, endState=$endState)"
}

/**
 * A value that animates (`androidx.compose.animation.core.Animatable`): [animateTo] runs one animation at a time (a new
 * one cancels the running one, keeping its velocity), frame by frame on the frame clock; [value] is snapshot state.
 */
class Animatable<T>(initialValue: T, val typeConverter: TwoWayConverter<T>, private val visibilityThreshold: T? = null, val label: String = "Animatable") {
    private val valueState = mutableStateOf(initialValue)
    private val targetState = mutableStateOf(initialValue)
    private val runningState = mutableStateOf(false)
    private var velocityVector = FloatArray(typeConverter.convertToVector(initialValue).size)
    private val mutex = MutatorMutex()
    private var lower: FloatArray? = null
    private var upper: FloatArray? = null

    val value: T get() = valueState.value
    val targetValue: T get() = targetState.value
    val isRunning: Boolean get() = runningState.value
    val velocity: T get() = typeConverter.convertFromVector(velocityVector)
    var lowerBound: T? = null
        private set
    var upperBound: T? = null
        private set

    private fun threshold(i: Int): Float =
        visibilityThreshold?.let { typeConverter.convertToVector(it).getOrNull(i) } ?: Spring.DefaultDisplacementThreshold

    private fun clamp(v: FloatArray): Pair<FloatArray, Boolean> {
        var hit = false
        val out = FloatArray(v.size) { i ->
            var x = v[i]
            lower?.getOrNull(i)?.let { if (x < it) { x = it; hit = true } }
            upper?.getOrNull(i)?.let { if (x > it) { x = it; hit = true } }
            x
        }
        return out to hit
    }

    fun updateBounds(lowerBound: T? = this.lowerBound, upperBound: T? = this.upperBound) {
        this.lowerBound = lowerBound
        this.upperBound = upperBound
        lower = lowerBound?.let(typeConverter.convertToVector)
        upper = upperBound?.let(typeConverter.convertToVector)
        val (c, _) = clamp(typeConverter.convertToVector(value))
        valueState.value = typeConverter.convertFromVector(c)
    }

    suspend fun snapTo(targetValue: T) {
        mutex.mutate {
            velocityVector = FloatArray(velocityVector.size)
            val (c, _) = clamp(typeConverter.convertToVector(targetValue))
            val v = typeConverter.convertFromVector(c)
            valueState.value = v
            targetState.value = v
        }
    }

    suspend fun stop() {
        mutex.mutate(MutatePriority.Default) {
            velocityVector = FloatArray(velocityVector.size)
            targetState.value = value
        }
    }

    suspend fun animateTo(
        targetValue: T,
        animationSpec: AnimationSpec<T> = SpringSpec(visibilityThreshold = visibilityThreshold),
        initialVelocity: T = velocity,
        block: (Animatable<T>.() -> Unit)? = null,
    ): AnimationResult<T> = mutex.mutate {
        targetState.value = targetValue
        val from = typeConverter.convertToVector(value)
        val to = typeConverter.convertToVector(targetValue)
        val v0 = typeConverter.convertToVector(initialVelocity)
        runningState.value = true
        try {
            var start = -1L
            var reason = AnimationEndReason.Finished
            while (true) {
                val now = withFrameNanos { it }
                if (start < 0) start = now
                val elapsed = now - start
                val (cur, vel, done) = sample(animationSpec, from, to, v0, elapsed)
                val (clamped, hit) = clamp(cur)
                velocityVector = vel
                valueState.value = typeConverter.convertFromVector(clamped)
                block?.invoke(this)
                if (hit) {
                    reason = AnimationEndReason.BoundReached
                    velocityVector = FloatArray(vel.size)
                    break
                }
                if (done) {
                    velocityVector = FloatArray(vel.size)
                    break
                }
            }
            AnimationResult(value, reason)
        } finally {
            runningState.value = false
        }
    }

    /** Decelerate from [initialVelocity] with [animationSpec] (float components independently). */
    suspend fun animateDecay(initialVelocity: T, animationSpec: DecayAnimationSpec<T>, block: (Animatable<T>.() -> Unit)? = null): AnimationResult<T> =
        mutex.mutate {
            val from = typeConverter.convertToVector(value)
            val v0 = typeConverter.convertToVector(initialVelocity)
            runningState.value = true
            try {
                var start = -1L
                var reason = AnimationEndReason.Finished
                val duration = v0.maxOf { animationSpec.durationNanos(it) }
                targetState.value = typeConverter.convertFromVector(FloatArray(from.size) { i -> animationSpec.targetValue(from[i], v0[i]) })
                while (true) {
                    val now = withFrameNanos { it }
                    if (start < 0) start = now
                    val t = (now - start).coerceAtMost(duration)
                    val cur = FloatArray(from.size) { i -> from[i] + animationSpec.valueChange(t.coerceAtMost(animationSpec.durationNanos(v0[i])), v0[i]) }
                    val vel = FloatArray(from.size) { i -> if (t >= animationSpec.durationNanos(v0[i])) 0f else animationSpec.velocityAt(t, v0[i]) }
                    val (clamped, hit) = clamp(cur)
                    velocityVector = vel
                    valueState.value = typeConverter.convertFromVector(clamped)
                    block?.invoke(this)
                    if (hit) {
                        reason = AnimationEndReason.BoundReached
                        break
                    }
                    if (t >= duration) break
                }
                velocityVector = FloatArray(from.size)
                AnimationResult(value, reason)
            } finally {
                runningState.value = false
            }
        }

    private fun sample(spec: AnimationSpec<T>, from: FloatArray, to: FloatArray, v0: FloatArray, elapsedNanos: Long): Triple<FloatArray, FloatArray, Boolean> =
        if (spec is KeyframesSpec<T>) {
            spec.sample(this, from, to, elapsedNanos / 1_000_000f)
        } else {
            sampleSpec(spec, from, to, v0, elapsedNanos) { i ->
                (spec as? SpringSpec<T>)?.visibilityThreshold?.let { typeConverter.convertToVector(it).getOrNull(i) } ?: threshold(i)
            }
        }
}

/**
 * Value, velocity and whether it ended, [elapsedNanos] into an animation from [from] to [to] (float components) with
 * [spec]; [threshold] gives each component's spring settle distance.
 */
internal fun sampleSpec(
    spec: AnimationSpec<*>,
    from: FloatArray,
    to: FloatArray,
    v0: FloatArray,
    elapsedNanos: Long,
    threshold: (Int) -> Float = { Spring.DefaultDisplacementThreshold },
): Triple<FloatArray, FloatArray, Boolean> {
    val ms = elapsedNanos / 1_000_000f
    return when (spec) {
        is TweenSpec<*> -> {
            val d = spec.durationMillis.coerceAtLeast(0)
            val p = if (d == 0) (if (ms >= spec.delay) 1f else 0f) else ((ms - spec.delay) / d).coerceIn(0f, 1f)
            val e = spec.easing.transform(p)
            val cur = FloatArray(from.size) { i -> from[i] + (to[i] - from[i]) * e }
            val dt = 0.016f
            val e2 = spec.easing.transform(((ms + dt * 1000 - spec.delay) / d.coerceAtLeast(1)).coerceIn(0f, 1f))
            val vel = FloatArray(from.size) { i -> if (p >= 1f) 0f else (to[i] - from[i]) * (e2 - e) / dt }
            Triple(cur, vel, ms >= spec.delay + d)
        }
        is SnapSpec<*> -> Triple(if (ms >= spec.delay) to.copyOf() else from.copyOf(), FloatArray(from.size), ms >= spec.delay)
        else -> {
            val s = spec as? SpringSpec<*> ?: SpringSpec<Float>()
            val t = ms / 1000f
            var settled = true
            val cur = FloatArray(from.size)
            val vel = FloatArray(from.size)
            for (i in from.indices) {
                val (x, v) = springAt(from[i] - to[i], v0.getOrElse(i) { 0f }, t, s.stiffness, s.dampingRatio)
                val th = threshold(i)
                if (abs(x) > th || abs(v) > th * 10f) settled = false
                cur[i] = to[i] + x
                vel[i] = v
            }
            if (settled) Triple(to.copyOf(), FloatArray(from.size), true) else Triple(cur, vel, false)
        }
    }
}

fun Animatable(initialValue: Float, visibilityThreshold: Float = Spring.DefaultDisplacementThreshold): Animatable<Float> =
    Animatable(initialValue, FloatConverter, visibilityThreshold)

/** Keyframes (`keyframes { durationMillis = ...; value at time }`). */
class KeyframesSpec<T> internal constructor(private val config: KeyframesSpecConfig<T>) : FiniteAnimationSpec<T> {
    internal fun sample(a: Animatable<T>, from: FloatArray, to: FloatArray, ms: Float): Triple<FloatArray, FloatArray, Boolean> {
        val d = config.durationMillis.toFloat().coerceAtLeast(1f)
        val t = (ms - config.delayMillis).coerceIn(0f, d)
        val frames = buildList {
            add(Triple(0f, from, LinearEasing))
            config.frames.sortedBy { it.first }.forEach { (time, v, e) -> add(Triple(time.toFloat(), a.typeConverter.convertToVector(v), e)) }
            add(Triple(d, to, LinearEasing))
        }
        val i = frames.indexOfLast { it.first <= t }.coerceIn(0, frames.size - 2)
        val (t0, v0, e) = frames[i]
        val (t1, v1, _) = frames[i + 1]
        val f = if (t1 > t0) e.transform(((t - t0) / (t1 - t0)).coerceIn(0f, 1f)) else 1f
        return Triple(FloatArray(from.size) { k -> v0[k] + (v1[k] - v0[k]) * f }, FloatArray(from.size), ms >= config.delayMillis + d)
    }
}

class KeyframesSpecConfig<T> {
    var durationMillis: Int = AnimationConstants.DefaultDurationMillis
    var delayMillis: Int = 0
    internal val frames = ArrayList<Triple<Int, T, Easing>>()

    inner class KeyframeEntity(private val index: Int) {
        infix fun using(easing: Easing): KeyframeEntity {
            val (t, v, _) = frames[index]
            frames[index] = Triple(t, v, easing)
            return this
        }
    }

    infix fun T.at(timeStamp: Int): KeyframeEntity {
        frames.add(Triple(timeStamp, this, LinearEasing))
        return KeyframeEntity(frames.size - 1)
    }

    infix fun T.atFraction(fraction: Float): KeyframeEntity = at((fraction * durationMillis).roundToInt())
}

fun <T> keyframes(init: KeyframesSpecConfig<T>.() -> Unit): KeyframesSpec<T> = KeyframesSpec(KeyframesSpecConfig<T>().apply(init))

// ---- animate*AsState -------------------------------------------------------------------------------------------------

private val defaultFloatSpring = spring<Float>()

@Composable
fun <T> animateValueAsState(
    targetValue: T,
    typeConverter: TwoWayConverter<T>,
    animationSpec: AnimationSpec<T> = remember { spring() },
    visibilityThreshold: T? = null,
    label: String = "ValueAnimation",
    finishedListener: ((T) -> Unit)? = null,
): State<T> {
    val animatable = remember { Animatable(targetValue, typeConverter, visibilityThreshold, label) }
    val listener by rememberUpdatedState(finishedListener)
    val resolved: AnimationSpec<T> =
        if (animationSpec is SpringSpec<T> && animationSpec.visibilityThreshold == null && visibilityThreshold != null) {
            SpringSpec(animationSpec.dampingRatio, animationSpec.stiffness, visibilityThreshold)
        } else {
            animationSpec
        }
    val spec by rememberUpdatedState(resolved)
    LaunchedEffect(targetValue) {
        if (targetValue != animatable.targetValue || animatable.value != targetValue) {
            animatable.animateTo(targetValue, spec)
            listener?.invoke(animatable.value)
        }
    }
    return remember { AnimatableState(animatable) }
}

private class AnimatableState<T>(private val a: Animatable<T>) : State<T> {
    override val value: T get() = a.value
}

@Composable
fun animateFloatAsState(
    targetValue: Float,
    animationSpec: AnimationSpec<Float> = defaultFloatSpring,
    visibilityThreshold: Float = 0.01f,
    label: String = "FloatAnimation",
    finishedListener: ((Float) -> Unit)? = null,
): State<Float> = animateValueAsState(targetValue, FloatConverter, animationSpec, visibilityThreshold, label, finishedListener)

@Composable
fun animateDpAsState(
    targetValue: Dp,
    animationSpec: AnimationSpec<Dp> = remember { spring(visibilityThreshold = Dp(0.1f)) },
    label: String = "DpAnimation",
    finishedListener: ((Dp) -> Unit)? = null,
): State<Dp> = animateValueAsState(targetValue, DpConverter, animationSpec, Dp(0.1f), label, finishedListener)

@Composable
fun animateIntAsState(
    targetValue: Int,
    animationSpec: AnimationSpec<Int> = remember { spring(visibilityThreshold = 1) },
    label: String = "IntAnimation",
    finishedListener: ((Int) -> Unit)? = null,
): State<Int> = animateValueAsState(targetValue, IntConverter, animationSpec, 1, label, finishedListener)

@Composable
fun animateOffsetAsState(
    targetValue: Offset,
    animationSpec: AnimationSpec<Offset> = remember { spring(visibilityThreshold = Offset(0.5f, 0.5f)) },
    label: String = "OffsetAnimation",
    finishedListener: ((Offset) -> Unit)? = null,
): State<Offset> = animateValueAsState(targetValue, OffsetConverter, animationSpec, Offset(0.5f, 0.5f), label, finishedListener)
