package androidx.compose.ui.graphics

import androidx.compose.foundation.wire
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.jsonNum

data class Shadow(val color: Color = Color.Black, val offset: Offset = Offset.Zero, val blurRadius: Float = 0f) {
    companion object { val None = Shadow(Color.Transparent) }
}

internal fun Shadow.wire(): String =
    "[${color.wire()},${jsonNum(offset.x)},${jsonNum(offset.y)},${jsonNum(blurRadius)}]"
