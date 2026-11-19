package androidx.compose.foundation.shape

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.jsonNum
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** A corner size: dp or percent of the shorter side. */
@Immutable
data class CornerSize internal constructor(val dp: Float?, val percent: Float?)

fun CornerSize(size: Dp) = CornerSize(size.value, null)
fun CornerSize(percent: Int) = CornerSize(null, percent.toFloat())
fun CornerSize(percent: Float) = CornerSize(null, percent)
val ZeroCornerSize = CornerSize(0.dp)

@Immutable
open class CornerBasedShape(val topStart: CornerSize, val topEnd: CornerSize, val bottomEnd: CornerSize, val bottomStart: CornerSize) : Shape {
    override fun wire(): String? {
        val all = listOf(topStart, topEnd, bottomEnd, bottomStart)
        if (all.all { it.percent != null }) return """{"percent":[${all.joinToString(",") { jsonNum(it.percent!!) }}]}"""
        // ponytail: a mix of dp and percent corners is sent as dp (percent corners read as 0).
        val dps = all.map { it.dp ?: 0f }
        if (dps.all { it == 0f }) return null
        return """{"corners":[${dps.joinToString(",") { jsonNum(it) }}]}"""
    }

    fun copy(topStart: CornerSize = this.topStart, topEnd: CornerSize = this.topEnd, bottomEnd: CornerSize = this.bottomEnd, bottomStart: CornerSize = this.bottomStart) =
        RoundedCornerShape(topStart, topEnd, bottomEnd, bottomStart)

    fun copy(all: CornerSize) = RoundedCornerShape(all, all, all, all)

    override fun equals(other: Any?) = other is CornerBasedShape && wire() == other.wire()
    override fun hashCode() = wire().hashCode()
}

class RoundedCornerShape(topStart: CornerSize, topEnd: CornerSize, bottomEnd: CornerSize, bottomStart: CornerSize) :
    CornerBasedShape(topStart, topEnd, bottomEnd, bottomStart)

fun RoundedCornerShape(corner: CornerSize) = RoundedCornerShape(corner, corner, corner, corner)
fun RoundedCornerShape(size: Dp) = RoundedCornerShape(CornerSize(size))
fun RoundedCornerShape(percent: Int) = RoundedCornerShape(CornerSize(percent))
fun RoundedCornerShape(topStart: Dp = 0.dp, topEnd: Dp = 0.dp, bottomEnd: Dp = 0.dp, bottomStart: Dp = 0.dp) =
    RoundedCornerShape(CornerSize(topStart), CornerSize(topEnd), CornerSize(bottomEnd), CornerSize(bottomStart))
fun RoundedCornerShape(topStartPercent: Int = 0, topEndPercent: Int = 0, bottomEndPercent: Int = 0, bottomStartPercent: Int = 0) =
    RoundedCornerShape(CornerSize(topStartPercent), CornerSize(topEndPercent), CornerSize(bottomEndPercent), CornerSize(bottomStartPercent))

val CircleShape = RoundedCornerShape(50)

/** Cut corners are drawn as rounded (the scene has no chamfer). */
// ponytail: CutCornerShape renders rounded.
fun CutCornerShape(size: Dp) = RoundedCornerShape(size)
