package androidx.compose.ui.platform

import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import onesim.compose.Guest

/** Touch thresholds and timeouts of the device (Pixel 10a, Android 17 defaults), in pixels and milliseconds. */
interface ViewConfiguration {
    val longPressTimeoutMillis: Long
    val doubleTapTimeoutMillis: Long
    val doubleTapMinTimeMillis: Long
    val touchSlop: Float
    val handwritingSlop: Float get() = 2f
    val minimumTouchTargetSize: DpSize get() = DpSize(48.dp, 48.dp)
    val maximumFlingVelocity: Float get() = Float.MAX_VALUE
    val handwritingGestureLineMargin: Float get() = 16f
}

/** Android's `ViewConfiguration` defaults: 8 dp touch slop, 400 ms long press, 300 ms double tap, 8000 dp/s fling. */
internal class AndroidViewConfiguration(private val density: Float) : ViewConfiguration {
    override val longPressTimeoutMillis: Long get() = 400L
    override val doubleTapTimeoutMillis: Long get() = 300L
    override val doubleTapMinTimeMillis: Long get() = 40L
    override val touchSlop: Float get() = kotlin.math.round(8f * density) // getScaledTouchSlop()
    override val handwritingSlop: Float get() = kotlin.math.round(2f * density)
    override val maximumFlingVelocity: Float get() = kotlin.math.round(8000f * density)
    override val handwritingGestureLineMargin: Float get() = kotlin.math.round(16f * density)
}

val LocalDensity = staticCompositionLocalOf<Density> { Guest.densityObject }
val LocalViewConfiguration = staticCompositionLocalOf<ViewConfiguration> { Guest.viewConfiguration }
