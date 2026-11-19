package androidx.compose.foundation

import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.ClickModifier
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.jsonNum
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.wire
import simcore.compose.Guest

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

fun Modifier.clickable(
    enabled: Boolean = true,
    onClickLabel: String? = null,
    role: Role? = null,
    onClick: () -> Unit,
): Modifier = this then ClickModifier(enabled, role?.toString(), null, onClick)

/** `interactionSource` / `indication` overload: ripples are not drawn. */
fun Modifier.clickable(
    interactionSource: Any?,
    indication: Any?,
    enabled: Boolean = true,
    onClickLabel: String? = null,
    role: Role? = null,
    onClick: () -> Unit,
): Modifier = clickable(enabled, onClickLabel, role, onClick)

fun Modifier.combinedClickable(
    enabled: Boolean = true,
    onClickLabel: String? = null,
    role: Role? = null,
    onLongClickLabel: String? = null,
    onLongClick: (() -> Unit)? = null,
    onDoubleClick: (() -> Unit)? = null,
    onClick: () -> Unit,
): Modifier = clickable(enabled, onClickLabel, role, onClick)

/** True while the session appearance is dark (`set_appearance` recomposes readers). */
@Composable
@ReadOnlyComposable
fun isSystemInDarkTheme(): Boolean = Guest.dark.value
