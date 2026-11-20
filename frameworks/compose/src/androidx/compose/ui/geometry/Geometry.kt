package androidx.compose.ui.geometry

data class Offset(val x: Float, val y: Float) {
    companion object {
        val Zero = Offset(0f, 0f)
        val Unspecified = Offset(Float.NaN, Float.NaN)
        val Infinite = Offset(Float.POSITIVE_INFINITY, Float.POSITIVE_INFINITY)
    }
}

data class Rect(val left: Float, val top: Float, val right: Float, val bottom: Float) {
    val width: Float get() = right - left
    val height: Float get() = bottom - top
    val center: Offset get() = Offset((left + right) / 2, (top + bottom) / 2)
    companion object { val Zero = Rect(0f, 0f, 0f, 0f) }
}


data class Size(val width: Float, val height: Float) {
    val minDimension: Float get() = minOf(width, height)
    val maxDimension: Float get() = maxOf(width, height)
    val center: Offset get() = Offset(width / 2f, height / 2f)
    companion object { val Zero = Size(0f, 0f) }
}

data class CornerRadius(val x: Float, val y: Float = x) {
    companion object { val Zero = CornerRadius(0f) }
}

data class RoundRect(
    val left: Float,
    val top: Float,
    val right: Float,
    val bottom: Float,
    val topLeftCornerRadius: CornerRadius = CornerRadius.Zero,
    val topRightCornerRadius: CornerRadius = CornerRadius.Zero,
    val bottomRightCornerRadius: CornerRadius = CornerRadius.Zero,
    val bottomLeftCornerRadius: CornerRadius = CornerRadius.Zero,
) {
    constructor(
        rect: Rect,
        topLeft: CornerRadius = CornerRadius.Zero,
        topRight: CornerRadius = CornerRadius.Zero,
        bottomRight: CornerRadius = CornerRadius.Zero,
        bottomLeft: CornerRadius = CornerRadius.Zero,
    ) : this(rect.left, rect.top, rect.right, rect.bottom, topLeft, topRight, bottomRight, bottomLeft)

    constructor(rect: Rect, cornerRadius: CornerRadius) : this(
        rect.left, rect.top, rect.right, rect.bottom, cornerRadius, cornerRadius, cornerRadius, cornerRadius,
    )
}
