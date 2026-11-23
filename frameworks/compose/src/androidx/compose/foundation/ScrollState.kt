package androidx.compose.foundation

import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.SpringSpec
import androidx.compose.foundation.gestures.ScrollScope
import androidx.compose.foundation.gestures.ScrollableState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.rememberSaveable
import kotlin.math.roundToInt

/**
 * Scroll position of a scrolling container, in pixels. OneSim's scrolling containers (scrollable tab rows) are scrolled
 * by the host, so this state holds the app's programmatic value; [maxValue] stays unbounded until a container reports
 * its size.
 */
@Stable
class ScrollState(initial: Int) : ScrollableState {
    private val valueState = mutableIntStateOf(initial)
    private val maxState = mutableIntStateOf(Int.MAX_VALUE)
    private val scrolling = mutableStateOf(false)
    private var accumulator = 0f

    val value: Int get() = valueState.intValue
    var maxValue: Int
        get() = maxState.intValue
        internal set(v) {
            maxState.intValue = v
            if (value > v) valueState.intValue = v
        }
    var viewportSize: Int = 0
        internal set

    override val isScrollInProgress: Boolean get() = scrolling.value
    override val canScrollForward: Boolean get() = value < maxValue
    override val canScrollBackward: Boolean get() = value > 0

    override fun dispatchRawDelta(delta: Float): Float {
        val absolute = value + delta + accumulator
        val new = absolute.coerceIn(0f, maxValue.toFloat())
        val consumed = new - value
        val rounded = consumed.roundToInt()
        valueState.intValue = value + rounded
        accumulator = consumed - rounded
        return consumed
    }

    override suspend fun scroll(scrollPriority: MutatePriority, block: suspend ScrollScope.() -> Unit) {
        scrolling.value = true
        try {
            object : ScrollScope {
                override fun scrollBy(pixels: Float): Float = dispatchRawDelta(pixels)
            }.block()
        } finally {
            scrolling.value = false
        }
    }

    suspend fun scrollTo(value: Int): Float = dispatchRawDelta((value - this.value).toFloat())

    suspend fun animateScrollTo(value: Int, animationSpec: AnimationSpec<Float> = SpringSpec()) {
        val a = androidx.compose.animation.core.Animatable(this.value.toFloat())
        scroll { a.animateTo(value.toFloat(), animationSpec) { scrollBy(this.value - this@ScrollState.value) } }
    }

    companion object {
        val Saver: Saver<ScrollState, *> = Saver(save = { it.value }, restore = { ScrollState(it) })
    }
}

@Composable
fun rememberScrollState(initial: Int = 0): ScrollState = rememberSaveable(saver = ScrollState.Saver) { ScrollState(initial) }
