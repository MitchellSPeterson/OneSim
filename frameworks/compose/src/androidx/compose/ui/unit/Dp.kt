package androidx.compose.ui.unit

import androidx.compose.runtime.Immutable

/** Density-independent pixels; OneSim's scene points. */
@Immutable
@JvmInline
value class Dp(val value: Float) : Comparable<Dp> {
    operator fun plus(other: Dp) = Dp(value + other.value)
    operator fun minus(other: Dp) = Dp(value - other.value)
    operator fun unaryMinus() = Dp(-value)
    operator fun times(other: Float) = Dp(value * other)
    operator fun times(other: Int) = Dp(value * other)
    operator fun div(other: Float) = Dp(value / other)
    operator fun div(other: Int) = Dp(value / other)
    operator fun div(other: Dp) = value / other.value
    override fun compareTo(other: Dp) = value.compareTo(other.value)
    override fun toString() = if (isUnspecified) "Dp.Unspecified" else "$value.dp"
    val isSpecified: Boolean get() = !value.isNaN()
    val isUnspecified: Boolean get() = value.isNaN()

    companion object {
        val Hairline = Dp(0f)
        val Infinity = Dp(Float.POSITIVE_INFINITY)
        val Unspecified = Dp(Float.NaN)
    }
}

inline val Int.dp: Dp get() = Dp(this.toFloat())
inline val Double.dp: Dp get() = Dp(this.toFloat())
inline val Float.dp: Dp get() = Dp(this)
operator fun Float.times(other: Dp) = Dp(this * other.value)
operator fun Int.times(other: Dp) = Dp(this * other.value)
fun max(a: Dp, b: Dp) = if (a >= b) a else b
fun min(a: Dp, b: Dp) = if (a <= b) a else b
fun Dp.coerceAtLeast(minimumValue: Dp) = max(this, minimumValue)
fun Dp.coerceAtMost(maximumValue: Dp) = min(this, maximumValue)
inline fun Dp.takeOrElse(block: () -> Dp): Dp = if (isSpecified) this else block()

/** Width and height in dp. */
@Immutable
data class DpSize(val width: Dp, val height: Dp) {
    companion object {
        val Zero = DpSize(0.dp, 0.dp)
        val Unspecified = DpSize(Dp.Unspecified, Dp.Unspecified)
    }
}

/** Scaled pixels (font scale 1: equal to dp) or em. */
@Immutable
@JvmInline
value class TextUnit(private val packed: Long) {
    val value: Float get() = Float.fromBits((packed and 0xffffffffL).toInt())
    val isSp: Boolean get() = (packed ushr 32) == 1L
    val isEm: Boolean get() = (packed ushr 32) == 2L
    val isUnspecified: Boolean get() = (packed ushr 32) == 0L
    val isSpecified: Boolean get() = !isUnspecified
    operator fun times(other: Float) = TextUnit((packed and 0xffffffffL.inv()) or ((value * other).toBits().toLong() and 0xffffffffL))
    override fun toString() = when { isSp -> "$value.sp"; isEm -> "$value.em"; else -> "Unspecified" }

    companion object {
        val Unspecified = TextUnit(0L)
        @PublishedApi internal fun of(type: Long, v: Float) = TextUnit((type shl 32) or (v.toBits().toLong() and 0xffffffffL))
    }
}

inline val Int.sp: TextUnit get() = TextUnit.of(1L, this.toFloat())
inline val Double.sp: TextUnit get() = TextUnit.of(1L, this.toFloat())
inline val Float.sp: TextUnit get() = TextUnit.of(1L, this)
inline val Int.em: TextUnit get() = TextUnit.of(2L, this.toFloat())
inline val Double.em: TextUnit get() = TextUnit.of(2L, this.toFloat())
inline val Float.em: TextUnit get() = TextUnit.of(2L, this)
inline fun TextUnit.takeOrElse(block: () -> TextUnit): TextUnit = if (isSpecified) this else block()

/** Font scale 1: one sp is one dp. */
interface Density {
    val density: Float
    val fontScale: Float get() = 1f
    fun Dp.toPx(): Float = value * density
    fun TextUnit.toDp(): Dp = Dp(value)
}
