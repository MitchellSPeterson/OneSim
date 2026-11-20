package androidx.compose.ui.graphics

import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.jsonNum
import kotlin.io.encoding.Base64

/** Raster canvas for painting measured text onto an image. */
class Canvas internal constructor(private val bitmap: BitmapStorage) {
    private data class Transform(val x: Float, val y: Float, val clip: Rect)
    private var state = Transform(0f, 0f, Rect(0f, 0f, bitmap.width.toFloat(), bitmap.height.toFloat()))
    private val saved = ArrayDeque<Transform>()

    fun save() {
        saved.addLast(state)
    }

    fun restore() {
        check(saved.isNotEmpty()) { "restore called without save" }
        state = saved.removeLast()
    }

    fun translate(dx: Float, dy: Float) {
        state = state.copy(x = state.x + dx, y = state.y + dy)
    }

    fun clipRect(rect: Rect, clipOp: ClipOp = ClipOp.Intersect) = clipRect(rect.left, rect.top, rect.right, rect.bottom, clipOp)

    fun clipRect(left: Float, top: Float, right: Float, bottom: Float, clipOp: ClipOp = ClipOp.Intersect) {
        require(clipOp == ClipOp.Intersect)
        state = state.copy(clip = with(state.clip) {
            Rect(maxOf(this.left, left + state.x), maxOf(this.top, top + state.y),
                minOf(this.right, right + state.x), minOf(this.bottom, bottom + state.y))
        })
    }

    internal fun textPaintWire(): String {
        val clip = with(state.clip) { listOf(left, top, right, bottom).joinToString(",") { jsonNum(it) } }
        return """{"width":${bitmap.width},"height":${bitmap.height},"origin":[${jsonNum(state.x)},${jsonNum(state.y)}],"clip":[$clip]}"""
    }

    internal fun compositeText(result: String) {
        val ink = Base64.decode(result)
        require(ink.size == bitmap.pixels.size * 4)
        for (i in bitmap.pixels.indices) {
            val at = i * 4
            val alpha = ink[at + 3].toInt() and 255
            if (alpha == 0) continue
            val dst = bitmap.pixels[i]
            val dstAlpha = dst ushr 24
            val remain = 255 - alpha
            val outAlpha = alpha + (dstAlpha * remain + 127) / 255
            var output = outAlpha
            for (channel in 0..2) {
                val d = (dst ushr ((2 - channel) * 8)) and 255
                val s = ink[at + channel].toInt() and 255
                val premul = s + (d * dstAlpha * remain + 32_512) / 65_025
                val out = ((premul * 255 + outAlpha / 2) / outAlpha).coerceAtMost(255)
                output = (output shl 8) or out
            }
            bitmap.pixels[i] = output
        }
    }
}

enum class ClipOp { Intersect }
enum class BlendMode { SrcOver }

fun Canvas(image: ImageBitmap): Canvas {
    require(image is BitmapStorage && image.mutable) { "Canvas requires a mutable ImageBitmap" }
    return Canvas(image)
}
