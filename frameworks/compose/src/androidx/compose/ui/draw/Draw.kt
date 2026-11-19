package androidx.compose.ui.draw

import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.jsonNum
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.wire

fun Modifier.clip(shape: Shape): Modifier = wire("""{"clip":${shape.wire() ?: "{}"}}""")

fun Modifier.alpha(alpha: Float): Modifier = wire("""{"alpha":${jsonNum(alpha)}}""")

/** `ambientColor` / `spotColor` are accepted but the host always draws Android's default black shadow. */
fun Modifier.shadow(
    elevation: Dp,
    shape: Shape = RectangleShape,
    clip: Boolean = elevation.value > 0f,
    ambientColor: Color = Color.Black,
    spotColor: Color = Color.Black,
): Modifier {
    if (elevation.value <= 0f && !clip) return this
    val s = shape.wire()?.let { ""","shape":$it""" } ?: ""
    val m = wire("""{"shadow":{"elevation":${jsonNum(elevation.value)}$s}}""")
    return if (clip) m.clip(shape) else m
}
