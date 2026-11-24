package androidx.compose.ui.graphics

import androidx.compose.runtime.Immutable
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.jsonNum
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection

@Immutable
interface Shape {
    fun createOutline(size: Size, layoutDirection: LayoutDirection, density: Density): Outline =
        Outline.Rectangle(Rect(0f, 0f, size.width, size.height))

    /** Geometry of the built-in shapes in dp or as a percent of the marker size. */
    fun wire(): String? = null
}

val RectangleShape: Shape = object : Shape {
    override fun toString() = "RectangleShape"
}

sealed class Outline {
    data class Rectangle(val rect: Rect) : Outline()
    data class Rounded(val roundRect: RoundRect) : Outline()
    data class Generic(val path: Path) : Outline()
}

internal fun Outline.wire(density: Float): String = when (this) {
    is Outline.Generic -> path.wire(density)
    is Outline.Rectangle -> Path().apply { addRect(rect) }.wire(density)
    is Outline.Rounded -> {
        val r = roundRect
        val bounds = listOf(r.left, r.top, r.right, r.bottom).joinToString(",") { jsonNum(it / density) }
        val radii = listOf(r.topLeftCornerRadius, r.topRightCornerRadius, r.bottomRightCornerRadius, r.bottomLeftCornerRadius)
            .joinToString(",") { "[${jsonNum(it.x / density)},${jsonNum(it.y / density)}]" }
        """{"roundedOutline":[$bounds],"cornerRadii":[$radii]}"""
    }
}
