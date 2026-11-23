package androidx.compose.ui.graphics

import androidx.compose.runtime.Immutable
import kotlin.math.pow

/** sRGB color with components 0...1. */
@Immutable
class Color(val red: Float, val green: Float, val blue: Float, val alpha: Float = 1f) {
    fun copy(alpha: Float = this.alpha, red: Float = this.red, green: Float = this.green, blue: Float = this.blue) =
        Color(red, green, blue, alpha)

    val isSpecified: Boolean get() = this !== Unspecified
    val isUnspecified: Boolean get() = this === Unspecified

    override fun equals(other: Any?) = other is Color && (this === other ||
        (this !== Unspecified && other !== Unspecified && red == other.red && green == other.green && blue == other.blue && alpha == other.alpha))
    override fun hashCode() = ((red.hashCode() * 31 + green.hashCode()) * 31 + blue.hashCode()) * 31 + alpha.hashCode()
    override fun toString() = "Color($red, $green, $blue, $alpha)"

    companion object {
        val Black = Color(0xFF000000)
        val DarkGray = Color(0xFF444444)
        val Gray = Color(0xFF888888)
        val LightGray = Color(0xFFCCCCCC)
        val White = Color(0xFFFFFFFF)
        val Red = Color(0xFFFF0000)
        val Green = Color(0xFF00FF00)
        val Blue = Color(0xFF0000FF)
        val Yellow = Color(0xFFFFFF00)
        val Cyan = Color(0xFF00FFFF)
        val Magenta = Color(0xFFFF00FF)
        val Transparent = Color(0f, 0f, 0f, 0f)
        val Unspecified = Color(0f, 0f, 0f, 0f)
    }
}

/** `Color(0xAARRGGBB)`. */
fun Color(color: Long): Color = Color(
    ((color shr 16) and 0xff) / 255f, ((color shr 8) and 0xff) / 255f, (color and 0xff) / 255f, ((color shr 24) and 0xff) / 255f,
)

fun Color(color: Int): Color = Color(color.toLong() and 0xffffffffL)

fun Color(red: Int, green: Int, blue: Int, alpha: Int = 0xFF): Color = Color(red / 255f, green / 255f, blue / 255f, alpha / 255f)

inline fun Color.takeOrElse(block: () -> Color): Color = if (isSpecified) this else block()

/** Source-over composite of `this` on `background`. */
fun Color.compositeOver(background: Color): Color {
    val a = alpha + background.alpha * (1 - alpha)
    if (a == 0f) return Color.Transparent
    fun ch(f: Float, b: Float) = (f * alpha + b * background.alpha * (1 - alpha)) / a
    return Color(ch(red, background.red), ch(green, background.green), ch(blue, background.blue), a)
}

fun lerp(start: Color, stop: Color, fraction: Float): Color = Color(
    start.red + (stop.red - start.red) * fraction, start.green + (stop.green - start.green) * fraction,
    start.blue + (stop.blue - start.blue) * fraction, start.alpha + (stop.alpha - start.alpha) * fraction,
)

fun Color.luminance(): Float {
    fun lin(c: Float) = if (c <= 0.04045f) c / 12.92f else ((c + 0.055f) / 1.055f).toDouble().pow(2.4).toFloat()
    return 0.2126f * lin(red) + 0.7152f * lin(green) + 0.0722f * lin(blue)
}

/** A color computed when it is drawn (lets a ripple follow state without recomposing). */
fun interface ColorProducer {
    operator fun invoke(): Color
}
