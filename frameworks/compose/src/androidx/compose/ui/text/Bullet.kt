package androidx.compose.ui.text

import androidx.compose.foundation.shape.CircleShape
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawscope.DrawStyle
import androidx.compose.ui.graphics.drawscope.Fill
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.em

data class Bullet(
    val shape: Shape,
    val width: TextUnit,
    val height: TextUnit,
    val padding: TextUnit,
    val brush: Brush? = null,
    val alpha: Float = Float.NaN,
    val drawStyle: DrawStyle = Fill,
) : AnnotatedString.Annotation {
    companion object {
        val DefaultIndentation = 1.em
        val DefaultSize = 0.25.em
        val DefaultPadding = 0.25.em
        val Default = Bullet(CircleShape, DefaultSize, DefaultSize, DefaultPadding)
    }
}
