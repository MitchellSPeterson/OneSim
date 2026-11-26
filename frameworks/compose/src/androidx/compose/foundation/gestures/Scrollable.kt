package androidx.compose.foundation.gestures

import androidx.compose.foundation.MutatePriority
import androidx.compose.foundation.MutatorMutex
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.unit.Velocity
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import onesim.compose.Guest
import onesim.compose.NodeElement
import onesim.compose.NodeState
import onesim.compose.WireCx
import kotlin.math.abs
import kotlin.math.exp
import kotlin.math.ln
import kotlin.math.sign

/** Receives scroll deltas inside [ScrollableState.scroll]. */
interface ScrollScope {
    /** Scroll by [pixels]; returns how much was consumed. */
    fun scrollBy(pixels: Float): Float
}

/** State of a [scrollable]: it consumes deltas in [dispatchRawDelta]; [scroll] runs a scroll (user, fling, programmatic). */
interface ScrollableState {
    suspend fun scroll(scrollPriority: MutatePriority = MutatePriority.Default, block: suspend ScrollScope.() -> Unit)

    fun dispatchRawDelta(delta: Float): Float

    val isScrollInProgress: Boolean
    val canScrollForward: Boolean get() = true
    val canScrollBackward: Boolean get() = true
    val lastScrolledForward: Boolean get() = false
    val lastScrolledBackward: Boolean get() = false
}

/** A [ScrollableState] whose [consumeScrollDelta] gets each delta (pixels) and returns how much it consumed. */
fun ScrollableState(consumeScrollDelta: (Float) -> Float): ScrollableState = DefaultScrollableState(consumeScrollDelta)

@Composable
fun rememberScrollableState(consumeScrollDelta: (Float) -> Float): ScrollableState {
    val lambdaState = rememberUpdatedState(consumeScrollDelta)
    return remember { ScrollableState { lambdaState.value.invoke(it) } }
}

/** Scroll by [value] pixels at once; returns what was consumed. */
suspend fun ScrollableState.scrollBy(value: Float): Float {
    var consumed = 0f
    scroll { consumed = scrollBy(value) }
    return consumed
}

/** Stop any running scroll (fling or programmatic) of lower or equal priority. */
suspend fun ScrollableState.stopScroll(scrollPriority: MutatePriority = MutatePriority.Default) {
    scroll(scrollPriority) {}
}

private class DefaultScrollableState(val onDelta: (Float) -> Float) : ScrollableState {
    private val scrollScope = object : ScrollScope {
        override fun scrollBy(pixels: Float): Float {
            if (pixels.isNaN()) return 0f
            val delta = onDelta(pixels)
            isLastScrollForwardState.value = delta > 0
            isLastScrollBackwardState.value = delta < 0
            return delta
        }
    }
    private val scrollMutex = MutatorMutex()
    private val isScrollingState = mutableStateOf(false)
    private val isLastScrollForwardState = mutableStateOf(false)
    private val isLastScrollBackwardState = mutableStateOf(false)

    override suspend fun scroll(scrollPriority: MutatePriority, block: suspend ScrollScope.() -> Unit): Unit = coroutineScope {
        scrollMutex.mutateWith(scrollScope, scrollPriority) {
            isScrollingState.value = true
            try {
                block()
            } finally {
                isScrollingState.value = false
            }
        }
    }

    override fun dispatchRawDelta(delta: Float): Float = onDelta(delta)

    override val isScrollInProgress: Boolean get() = isScrollingState.value
    override val lastScrolledForward: Boolean get() = isLastScrollForwardState.value
    override val lastScrolledBackward: Boolean get() = isLastScrollBackwardState.value
}

/** What happens after the finger lifts with a velocity. */
@Stable
interface FlingBehavior {
    /** Scroll with [initialVelocity] (pixels per second); returns the velocity left over. */
    suspend fun ScrollScope.performFling(initialVelocity: Float): Float
}

object ScrollableDefaults {
    /** Android's fling: decelerates along the platform's spline (`OverScroller`). */
    @Composable
    fun flingBehavior(): FlingBehavior = remember { DefaultFlingBehavior(FlingCalculator(Guest.density)) }
}

/** Android's fling deceleration (`OverScroller` / `SplineBasedFloatDecayAnimationSpec`). */
internal class FlingCalculator(density: Float, private val friction: Float = 0.015f) {
    private val physicalCoeff = 9.80665f * 39.37f * density * 160f * 0.84f

    private fun splineDeceleration(velocity: Float): Double = ln(Inflection * abs(velocity) / (friction * physicalCoeff).toDouble())

    /** Duration (ms) of a fling at [velocity]. */
    fun flingDuration(velocity: Float): Long = (1000.0 * exp(splineDeceleration(velocity) / (DecelerationRate - 1.0))).toLong()

    /** Distance (pixels, signed) of a fling at [velocity]. */
    fun flingDistance(velocity: Float): Float =
        (friction * physicalCoeff * exp(DecelerationRate / (DecelerationRate - 1.0) * splineDeceleration(velocity))).toFloat() * sign(velocity)

    /** Offset reached [timeMs] into a fling at [velocity]. */
    fun position(velocity: Float, timeMs: Long): Float {
        val duration = flingDuration(velocity)
        val t = if (duration > 0) (timeMs.toFloat() / duration).coerceIn(0f, 1f) else 1f
        return flingDistance(velocity) * AndroidFlingSpline.flingPosition(t).first
    }

    /** Velocity (pixels per second) [timeMs] into a fling at [velocity]. */
    fun velocity(velocity: Float, timeMs: Long): Float {
        val duration = flingDuration(velocity)
        if (duration <= 0 || timeMs >= duration) return 0f
        val t = timeMs.toFloat() / duration
        return AndroidFlingSpline.flingPosition(t).second * flingDistance(velocity) / duration * 1000f
    }

    companion object {
        private const val Inflection = 0.35
        private val DecelerationRate = ln(0.78) / ln(0.9)
    }
}

/** Android's precomputed fling spline (`OverScroller.SplineOverScroller`). */
internal object AndroidFlingSpline {
    private const val NbSamples = 100
    private val SplinePositions = FloatArray(NbSamples + 1)

    init {
        val startTension = 0.5f
        val endTension = 1.0f
        val p1 = startTension * 0.35f
        val p2 = 1.0f - endTension * (1.0f - 0.35f)
        var xMin = 0.0f
        for (i in 0 until NbSamples) {
            val alpha = i.toFloat() / NbSamples
            var xMax = 1.0f
            var x: Float
            var tx: Float
            var coef: Float
            while (true) {
                x = xMin + (xMax - xMin) / 2.0f
                coef = 3.0f * x * (1.0f - x)
                tx = coef * ((1.0f - x) * p1 + x * p2) + x * x * x
                if (abs(tx - alpha) < 1E-5) break
                if (tx > alpha) xMax = x else xMin = x
            }
            SplinePositions[i] = coef * ((1.0f - x) * startTension + x * endTension) + x * x * x
        }
        SplinePositions[NbSamples] = 1.0f
    }

    /** (distance coefficient, velocity coefficient) at [time] in 0...1 of the fling. */
    fun flingPosition(time: Float): Pair<Float, Float> {
        val index = (NbSamples * time).toInt()
        if (index >= NbSamples) return 1f to 0f
        val tInf = index.toFloat() / NbSamples
        val tSup = (index + 1).toFloat() / NbSamples
        val dInf = SplinePositions[index]
        val dSup = SplinePositions[index + 1]
        val velocityCoef = (dSup - dInf) / (tSup - tInf)
        return (dInf + (time - tInf) * velocityCoef) to velocityCoef
    }
}

/** Decelerating fling, frame by frame; stops early when the state stops consuming. */
internal class DefaultFlingBehavior(private val calculator: FlingCalculator) : FlingBehavior {
    override suspend fun ScrollScope.performFling(initialVelocity: Float): Float {
        if (abs(initialVelocity) <= 1f) return initialVelocity
        var velocityLeft = initialVelocity
        var lastValue = 0f
        var start = -1L
        val duration = calculator.flingDuration(initialVelocity)
        while (true) {
            val frame = withFrameNanos { it }
            if (start < 0) start = frame
            val t = (frame - start) / 1_000_000L
            val value = calculator.position(initialVelocity, t)
            val delta = value - lastValue
            val consumed = scrollBy(delta)
            lastValue = value
            velocityLeft = calculator.velocity(initialVelocity, t)
            if (abs(delta - consumed) > 0.5f || t >= duration) break
        }
        return if (lastValue == calculator.position(initialVelocity, duration)) 0f else velocityLeft
    }
}

/**
 * Scroll along [orientation] by dragging: past the touch slop each move's delta goes to [state]
 * ([ScrollableState.dispatchRawDelta] semantics inside a [MutatePriority.UserInput] scroll), then [flingBehavior]
 * continues with the release velocity. A touch on a flinging scrollable catches it. Nested scrolling and overscroll
 * effects are not modelled.
 */
fun Modifier.scrollable(
    state: ScrollableState,
    orientation: Orientation,
    enabled: Boolean = true,
    reverseDirection: Boolean = false,
    flingBehavior: FlingBehavior? = null,
    interactionSource: MutableInteractionSource? = null,
): Modifier = this then ScrollableElement(state, orientation, enabled, reverseDirection, flingBehavior, interactionSource)

private class ScrollableElement(
    val state: ScrollableState,
    val orientation: Orientation,
    val enabled: Boolean,
    val reverseDirection: Boolean,
    val flingBehavior: FlingBehavior?,
    val interactionSource: MutableInteractionSource?,
) : NodeElement {
    override fun create(): NodeState = ScrollableNodeState()

    override fun wire(state: NodeState, cx: WireCx) {
        val s = state as ScrollableNodeState
        s.element = this
        s.orientationLock = orientation
        s.enabled = enabled
        s.interactionSource = interactionSource
        if (enabled) cx.markScrollContainer()
        s.wireGesture(cx)
    }
}

private class ScrollableNodeState : DragGestureState() {
    lateinit var element: ScrollableElement
    private val defaultFling by lazy { DefaultFlingBehavior(FlingCalculator(Guest.density)) }

    private fun Float.reverseIfNeeded() = if (element.reverseDirection) -this else this

    override fun startDragImmediately() = element.state.isScrollInProgress

    override suspend fun drag(forEachDelta: suspend ((DragEvent.DragDelta) -> Unit) -> Unit) {
        val e = element
        e.state.scroll(MutatePriority.UserInput) {
            forEachDelta { scrollBy(it.delta.along(e.orientation).reverseIfNeeded()) }
        }
    }

    override fun onDragStarted(startedPosition: Offset) {}

    override fun onDragStopped(velocity: Velocity) {
        val e = element
        val v = velocity.along(e.orientation).reverseIfNeeded()
        val fling = e.flingBehavior ?: defaultFling
        Guest.scope.launch {
            e.state.scroll(MutatePriority.Default) { with(fling) { performFling(v) } }
        }
    }
}
