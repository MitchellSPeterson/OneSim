package androidx.compose.ui.graphics.drawscope

import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.wire
import androidx.compose.ui.jsonNum
import androidx.compose.ui.jsonString

sealed class DrawStyle
data object Fill : DrawStyle()

data class Stroke(
    val width: Float = HairlineWidth,
    val miter: Float = DefaultMiter,
    val cap: StrokeCap = DefaultCap,
    val join: StrokeJoin = DefaultJoin,
    val pathEffect: PathEffect? = null,
) : DrawStyle() {
    companion object {
        const val HairlineWidth = 0f
        const val DefaultMiter = 4f
        val DefaultCap = StrokeCap.Butt
        val DefaultJoin = StrokeJoin.Miter
    }
}

internal fun Stroke.wire(): String = buildString {
    append("{\"width\":").append(jsonNum(width)).append(",\"miter\":").append(jsonNum(miter))
    append(",\"cap\":").append(jsonString(cap.wire)).append(",\"join\":").append(jsonString(join.wire))
    pathEffect?.let { append(',').append(it.wire()) }
    append('}')
}
