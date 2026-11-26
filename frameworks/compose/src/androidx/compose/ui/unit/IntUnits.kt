package androidx.compose.ui.unit

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.Stable
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import kotlin.math.roundToInt

/** Integer width and height in pixels. */
@Immutable
data class IntSize(val width: Int, val height: Int) {
    @Stable operator fun times(other: Int) = IntSize(width * other, height * other)
    @Stable operator fun div(other: Int) = IntSize(width / other, height / other)
    override fun toString() = "$width x $height"

    companion object {
        val Zero = IntSize(0, 0)
    }
}

@Stable val IntSize.center: IntOffset get() = IntOffset(width / 2, height / 2)
@Stable fun IntSize.toSize() = Size(width.toFloat(), height.toFloat())
@Stable fun Size.toIntSize() = IntSize(width.roundToInt(), height.roundToInt())

/** An integer offset in pixels (`Modifier.offset { }`). */
@Immutable
data class IntOffset(val x: Int, val y: Int) {
    @Stable operator fun minus(other: IntOffset) = IntOffset(x - other.x, y - other.y)
    @Stable operator fun plus(other: IntOffset) = IntOffset(x + other.x, y + other.y)
    @Stable operator fun unaryMinus() = IntOffset(-x, -y)
    @Stable operator fun times(operand: Float) = IntOffset((x * operand).roundToInt(), (y * operand).roundToInt())
    override fun toString() = "($x, $y)"

    companion object {
        val Zero = IntOffset(0, 0)
    }
}

@Stable fun IntOffset.toOffset() = Offset(x.toFloat(), y.toFloat())
@Stable fun Offset.round() = IntOffset(x.roundToInt(), y.roundToInt())

/** A two-dimensional velocity in pixels per second. */
@Immutable
data class Velocity(val x: Float, val y: Float) {
    @Stable operator fun minus(other: Velocity) = Velocity(x - other.x, y - other.y)
    @Stable operator fun plus(other: Velocity) = Velocity(x + other.x, y + other.y)
    @Stable operator fun unaryMinus() = Velocity(-x, -y)
    @Stable operator fun times(operand: Float) = Velocity(x * operand, y * operand)
    @Stable operator fun div(operand: Float) = Velocity(x / operand, y / operand)
    override fun toString() = "($x, $y) px/sec"

    companion object {
        val Zero = Velocity(0f, 0f)
    }
}

@Stable fun Velocity(offset: Offset) = Velocity(offset.x, offset.y)
