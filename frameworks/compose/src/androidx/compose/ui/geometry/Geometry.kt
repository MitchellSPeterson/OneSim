package androidx.compose.ui.geometry

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.Stable
import kotlin.math.sqrt

/** A 2D point or vector in pixels. [Unspecified] has NaN components. */
@Immutable
class Offset(val x: Float, val y: Float) {
    operator fun component1() = x
    operator fun component2() = y
    fun copy(x: Float = this.x, y: Float = this.y) = Offset(x, y)

    @Stable fun getDistance(): Float = sqrt(x * x + y * y)
    @Stable fun getDistanceSquared(): Float = x * x + y * y
    @Stable fun isValid(): Boolean = !x.isNaN() && !y.isNaN()
    @Stable operator fun unaryMinus() = Offset(-x, -y)
    @Stable operator fun minus(other: Offset) = Offset(x - other.x, y - other.y)
    @Stable operator fun plus(other: Offset) = Offset(x + other.x, y + other.y)
    @Stable operator fun times(operand: Float) = Offset(x * operand, y * operand)
    @Stable operator fun div(operand: Float) = Offset(x / operand, y / operand)
    @Stable operator fun rem(operand: Float) = Offset(x % operand, y % operand)

    override fun equals(other: Any?) = other is Offset &&
        (x == other.x || (x.isNaN() && other.x.isNaN())) && (y == other.y || (y.isNaN() && other.y.isNaN()))
    override fun hashCode() = x.hashCode() * 31 + y.hashCode()
    override fun toString() = if (isSpecified) "Offset(${x.r1()}, ${y.r1()})" else "Offset.Unspecified"

    companion object {
        val Zero = Offset(0f, 0f)
        val Infinite = Offset(Float.POSITIVE_INFINITY, Float.POSITIVE_INFINITY)
        val Unspecified = Offset(Float.NaN, Float.NaN)
    }
}

private fun Float.r1(): String = ((this * 10f).toInt() / 10f).toString()

@Stable val Offset.isSpecified: Boolean get() = !x.isNaN() || !y.isNaN()
@Stable val Offset.isUnspecified: Boolean get() = x.isNaN() && y.isNaN()
@Stable val Offset.isFinite: Boolean get() = x.isFinite() && y.isFinite()
inline fun Offset.takeOrElse(block: () -> Offset): Offset = if (isSpecified) this else block()
@Stable fun lerp(start: Offset, stop: Offset, fraction: Float) =
    Offset(start.x + (stop.x - start.x) * fraction, start.y + (stop.y - start.y) * fraction)

/** Width and height in pixels. */
@Immutable
class Size(val width: Float, val height: Float) {
    operator fun component1() = width
    operator fun component2() = height
    fun copy(width: Float = this.width, height: Float = this.height) = Size(width, height)

    @Stable val minDimension: Float get() = kotlin.math.min(kotlin.math.abs(width), kotlin.math.abs(height))
    @Stable val maxDimension: Float get() = kotlin.math.max(kotlin.math.abs(width), kotlin.math.abs(height))
    @Stable fun isEmpty() = width <= 0f || height <= 0f
    @Stable operator fun times(operand: Float) = Size(width * operand, height * operand)
    @Stable operator fun div(operand: Float) = Size(width / operand, height / operand)

    override fun equals(other: Any?) = other is Size && width == other.width && height == other.height
    override fun hashCode() = width.hashCode() * 31 + height.hashCode()
    override fun toString() = if (width.isNaN()) "Size.Unspecified" else "Size(${width.r1()}, ${height.r1()})"

    companion object {
        val Zero = Size(0f, 0f)
        val Unspecified = Size(Float.NaN, Float.NaN)
    }
}

@Stable val Size.center: Offset get() = Offset(width / 2f, height / 2f)
@Stable val Size.isSpecified: Boolean get() = !width.isNaN()
@Stable val Size.isUnspecified: Boolean get() = width.isNaN()
fun Size.toRect(): Rect = Rect(0f, 0f, width, height)

/** An axis-aligned rectangle in pixels. */
@Immutable
data class Rect(val left: Float, val top: Float, val right: Float, val bottom: Float) {
    val width: Float get() = right - left
    val height: Float get() = bottom - top
    val size: Size get() = Size(width, height)
    val center: Offset get() = Offset(left + width / 2f, top + height / 2f)
    val topLeft: Offset get() = Offset(left, top)
    val bottomRight: Offset get() = Offset(right, bottom)
    val isEmpty: Boolean get() = left >= right || top >= bottom
    operator fun contains(offset: Offset) = offset.x >= left && offset.x < right && offset.y >= top && offset.y < bottom
    fun translate(offset: Offset) = Rect(left + offset.x, top + offset.y, right + offset.x, bottom + offset.y)
    fun inflate(delta: Float) = Rect(left - delta, top - delta, right + delta, bottom + delta)

    companion object {
        val Zero = Rect(0f, 0f, 0f, 0f)
    }
}

fun Rect(offset: Offset, size: Size) = Rect(offset.x, offset.y, offset.x + size.width, offset.y + size.height)
