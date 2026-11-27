package androidx.compose.animation

import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.ColorConverter
import androidx.compose.animation.core.DecayAnimationSpec
import androidx.compose.animation.core.animateValueAsState
import androidx.compose.animation.core.spring
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import onesim.compose.Guest

@Composable
fun animateColorAsState(
    targetValue: Color,
    animationSpec: AnimationSpec<Color> = remember { spring() },
    label: String = "ColorAnimation",
    finishedListener: ((Color) -> Unit)? = null,
): State<Color> = animateValueAsState(targetValue, ColorConverter, animationSpec, null, label, finishedListener)

/** Android's fling deceleration (`OverScroller` spline), in pixels. */
@Composable
fun <T> rememberSplineBasedDecay(): DecayAnimationSpec<T> = remember(Guest.density) { splineBasedDecay(Guest.density) }

fun <T> splineBasedDecay(density: Float): DecayAnimationSpec<T> = SplineDecay(density)

/** Android's fling spline (`OverScroller`), shared with `scrollable` flings. */
private class SplineDecay<T>(density: Float) : DecayAnimationSpec<T> {
    private val calc = androidx.compose.foundation.gestures.FlingCalculator(density)

    override fun durationNanos(initialVelocity: Float): Long =
        if (initialVelocity == 0f) 0L else calc.flingDuration(initialVelocity) * 1_000_000L

    override fun valueChange(playTimeNanos: Long, initialVelocity: Float): Float =
        if (initialVelocity == 0f) 0f else calc.position(initialVelocity, playTimeNanos / 1_000_000L)

    override fun velocityAt(playTimeNanos: Long, initialVelocity: Float): Float =
        if (initialVelocity == 0f) 0f else calc.velocity(initialVelocity, playTimeNanos / 1_000_000L)
}
