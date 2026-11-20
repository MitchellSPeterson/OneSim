package androidx.compose.ui.graphics

import androidx.compose.ui.graphics.colorspace.ColorSpace
import androidx.compose.ui.graphics.colorspace.ColorSpaces

interface ImageBitmap {
    val width: Int
    val height: Int
    val colorSpace: ColorSpace
    val hasAlpha: Boolean
    val config: ImageBitmapConfig

    fun readPixels(
        buffer: IntArray,
        startX: Int = 0,
        startY: Int = 0,
        width: Int = this.width,
        height: Int = this.height,
        bufferOffset: Int = 0,
        stride: Int = width,
    )

    fun prepareToDraw()
    companion object
}

enum class ImageBitmapConfig { Argb8888, Alpha8, Rgb565, F16, Gpu }

fun ImageBitmap(
    width: Int,
    height: Int,
    config: ImageBitmapConfig = ImageBitmapConfig.Argb8888,
    hasAlpha: Boolean = true,
    colorSpace: ColorSpace = ColorSpaces.Srgb,
): ImageBitmap {
    require(width > 0 && height > 0 && width.toLong() * height <= 16_777_216L)
    require(config == ImageBitmapConfig.Argb8888 && colorSpace == ColorSpaces.Srgb) { "Expected an ARGB_8888 sRGB bitmap" }
    val initial = if (hasAlpha) 0 else 0xff000000.toInt()
    return BitmapStorage(width, height, hasAlpha, IntArray(width * height) { initial }, mutable = true)
}

internal class BitmapStorage(
    override val width: Int,
    override val height: Int,
    override val hasAlpha: Boolean,
    internal val pixels: IntArray,
    internal val mutable: Boolean = false,
) : ImageBitmap {
    override val colorSpace = ColorSpaces.Srgb
    override val config = ImageBitmapConfig.Argb8888

    override fun readPixels(buffer: IntArray, startX: Int, startY: Int, width: Int, height: Int, bufferOffset: Int, stride: Int) {
        require(startX >= 0 && startY >= 0 && width >= 0 && height >= 0)
        require(startX <= this.width - width && startY <= this.height - height)
        require(stride >= width && bufferOffset >= 0)
        if (height == 0 || width == 0) return
        require(bufferOffset.toLong() + stride.toLong() * (height - 1) + width <= buffer.size)
        for (y in 0 until height) {
            val row = (startY + y) * this.width + startX
            pixels.copyInto(buffer, bufferOffset + y * stride, row, row + width)
        }
    }

    override fun prepareToDraw() {}
}
