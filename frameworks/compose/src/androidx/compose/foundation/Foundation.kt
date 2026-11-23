package androidx.compose.foundation

import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.jsonNum
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.wire
import onesim.compose.Guest

internal fun Color.wire() = "[${jsonNum(red)},${jsonNum(green)},${jsonNum(blue)},${jsonNum(alpha)}]"

fun Modifier.background(color: Color, shape: Shape = RectangleShape): Modifier {
    if (color.isUnspecified || color.alpha == 0f) return this
    val s = shape.wire()?.let { ""","shape":$it""" } ?: ""
    return wire("""{"background":{"color":${color.wire()}$s}}""")
}

@Immutable
data class BorderStroke(val width: Dp, val color: Color)

fun Modifier.border(width: Dp, color: Color, shape: Shape = RectangleShape): Modifier {
    val s = shape.wire()?.let { ""","shape":$it""" } ?: ""
    return wire("""{"border":{"width":${jsonNum(width.value)},"color":${color.wire()}$s}}""")
}

fun Modifier.border(border: BorderStroke, shape: Shape = RectangleShape): Modifier = border(border.width, border.color, shape)

/** True while the session appearance is dark (`set_appearance` recomposes readers). */
@Composable
@ReadOnlyComposable
fun isSystemInDarkTheme(): Boolean = Guest.dark.value
