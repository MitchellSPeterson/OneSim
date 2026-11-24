package androidx.compose.ui.graphics

import androidx.compose.foundation.wire
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.jsonNum
import kotlin.jvm.JvmInline

@JvmInline
value class TileMode private constructor(internal val wire: Int) {
    companion object {
        val Clamp = TileMode(0)
        val Repeated = TileMode(1)
        val Mirror = TileMode(2)
        val Decal = TileMode(3)
    }
}

sealed class Brush {
    companion object {
        fun linearGradient(
            colors: List<Color>,
            start: Offset = Offset.Zero,
            end: Offset = Offset.Infinite,
            tileMode: TileMode = TileMode.Clamp,
        ): Brush = linearGradient(*colorStops(colors), start = start, end = end, tileMode = tileMode)

        fun linearGradient(
            vararg colorStops: Pair<Float, Color>,
            start: Offset = Offset.Zero,
            end: Offset = Offset.Infinite,
            tileMode: TileMode = TileMode.Clamp,
        ): Brush = gradient("linear", colorStops.toList(), listOf(start.x, start.y, end.x, end.y), tileMode)

        fun horizontalGradient(
            colors: List<Color>,
            startX: Float = 0f,
            endX: Float = Float.POSITIVE_INFINITY,
            tileMode: TileMode = TileMode.Clamp,
        ): Brush = horizontalGradient(*colorStops(colors), startX = startX, endX = endX, tileMode = tileMode)

        fun horizontalGradient(
            vararg colorStops: Pair<Float, Color>,
            startX: Float = 0f,
            endX: Float = Float.POSITIVE_INFINITY,
            tileMode: TileMode = TileMode.Clamp,
        ): Brush = linearGradient(*colorStops, start = Offset(startX, 0f), end = Offset(endX, 0f), tileMode = tileMode)

        fun verticalGradient(
            colors: List<Color>,
            startY: Float = 0f,
            endY: Float = Float.POSITIVE_INFINITY,
            tileMode: TileMode = TileMode.Clamp,
        ): Brush = verticalGradient(*colorStops(colors), startY = startY, endY = endY, tileMode = tileMode)

        fun verticalGradient(
            vararg colorStops: Pair<Float, Color>,
            startY: Float = 0f,
            endY: Float = Float.POSITIVE_INFINITY,
            tileMode: TileMode = TileMode.Clamp,
        ): Brush = linearGradient(*colorStops, start = Offset(0f, startY), end = Offset(0f, endY), tileMode = tileMode)

        fun radialGradient(
            colors: List<Color>,
            center: Offset = Offset.Unspecified,
            radius: Float = Float.POSITIVE_INFINITY,
            tileMode: TileMode = TileMode.Clamp,
        ): Brush = radialGradient(*colorStops(colors), center = center, radius = radius, tileMode = tileMode)

        fun radialGradient(
            vararg colorStops: Pair<Float, Color>,
            center: Offset = Offset.Unspecified,
            radius: Float = Float.POSITIVE_INFINITY,
            tileMode: TileMode = TileMode.Clamp,
        ): Brush = gradient("radial", colorStops.toList(), listOf(center.x, center.y, radius), tileMode)

        fun sweepGradient(colors: List<Color>, center: Offset = Offset.Unspecified): Brush =
            sweepGradient(*colorStops(colors), center = center)

        fun sweepGradient(vararg colorStops: Pair<Float, Color>, center: Offset = Offset.Unspecified): Brush =
            gradient("sweep", colorStops.toList(), listOf(center.x, center.y), TileMode.Clamp)
    }
}

data class SolidColor(val value: Color) : Brush()

abstract class ShaderBrush : Brush() {
    abstract fun createShader(size: Size): Shader
}

class Shader internal constructor(internal val wire: String)

fun ShaderBrush(shader: Shader): ShaderBrush = GradientBrush(shader)

fun LinearGradientShader(
    from: Offset, to: Offset, colors: List<Color>, colorStops: List<Float>? = null,
    tileMode: TileMode = TileMode.Clamp,
): Shader = shader("linear", colorStops(colors, colorStops).toList(), listOf(from.x, from.y, to.x, to.y), tileMode)

fun RadialGradientShader(
    center: Offset, radius: Float, colors: List<Color>, colorStops: List<Float>? = null,
    tileMode: TileMode = TileMode.Clamp,
): Shader = shader("radial", colorStops(colors, colorStops).toList(), listOf(center.x, center.y, radius), tileMode)

fun SweepGradientShader(center: Offset, colors: List<Color>, colorStops: List<Float>? = null): Shader =
    shader("sweep", colorStops(colors, colorStops).toList(), listOf(center.x, center.y), TileMode.Clamp)

fun ImageShader(
    image: ImageBitmap,
    tileModeX: TileMode = TileMode.Clamp,
    tileModeY: TileMode = TileMode.Clamp,
): Shader {
    val area = image.width.toLong() * image.height
    require(image.width > 0 && image.height > 0 && area <= Int.MAX_VALUE / 4)
    val pixels = IntArray(area.toInt())
    image.readPixels(pixels)
    val rgba = ByteArray(pixels.size * 4)
    for (i in pixels.indices) {
        val argb = pixels[i]
        rgba[i * 4] = (argb ushr 16).toByte()
        rgba[i * 4 + 1] = (argb ushr 8).toByte()
        rgba[i * 4 + 2] = argb.toByte()
        rgba[i * 4 + 3] = (argb ushr 24).toByte()
    }
    val encoded = kotlin.io.encoding.Base64.encode(rgba)
    return Shader(
        """{"kind":"image","image":{"width":${image.width},"height":${image.height},"tileX":${tileModeX.wire},"tileY":${tileModeY.wire},"rgba":"$encoded"}}""",
    )
}

private class GradientBrush(val shader: Shader) : ShaderBrush() {
    override fun createShader(size: Size) = shader
    override fun equals(other: Any?) = other is GradientBrush && other.shader.wire == shader.wire
    override fun hashCode() = shader.wire.hashCode()
}

internal fun Brush.wire(size: Size?): String? = when (this) {
    is SolidColor -> """{"kind":"solid","stops":[[0,${value.wire()}]]}"""
    is GradientBrush -> shader.wire
    is ShaderBrush -> size?.let { createShader(it).wire }
}

internal val Brush.needsSize: Boolean get() = this is ShaderBrush && this !is GradientBrush

private fun colorStops(colors: List<Color>, stops: List<Float>? = null): Array<Pair<Float, Color>> {
    require(stops == null || stops.size == colors.size)
    return colors.mapIndexed { index, color -> (stops?.get(index) ?: index.toFloat() / (colors.size - 1)) to color }.toTypedArray()
}

private fun gradient(kind: String, stops: List<Pair<Float, Color>>, positions: List<Float>, tileMode: TileMode): Brush =
    GradientBrush(shader(kind, stops, positions, tileMode))

private fun shader(kind: String, stops: List<Pair<Float, Color>>, positions: List<Float>, tileMode: TileMode): Shader {
    require(stops.size >= 2 && stops.all { it.first in 0f..1f } && stops.zipWithNext().all { (a, b) -> a.first <= b.first })
    val coordinates = positions.joinToString(prefix = "[", postfix = "]") {
        if (it == Float.NEGATIVE_INFINITY) "0" else if (it.isFinite()) jsonNum(it) else "null"
    }
    val wireStops = stops.joinToString(prefix = "[", postfix = "]") { (position, color) ->
        "[${jsonNum(position)},${color.wire()}]"
    }
    return Shader("""{"kind":"$kind","positions":$coordinates,"stops":$wireStops,"tile":${tileMode.wire}}""")
}
