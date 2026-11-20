package androidx.compose.ui.graphics

import androidx.compose.ui.jsonNum
import kotlin.jvm.JvmInline

@JvmInline
value class StrokeCap private constructor(internal val wire: String) {
    companion object {
        val Butt = StrokeCap("butt")
        val Round = StrokeCap("round")
        val Square = StrokeCap("square")
    }
}

@JvmInline
value class StrokeJoin private constructor(internal val wire: String) {
    companion object {
        val Miter = StrokeJoin("miter")
        val Round = StrokeJoin("round")
        val Bevel = StrokeJoin("bevel")
    }
}

@JvmInline
value class StampedPathEffectStyle private constructor(internal val wire: String) {
    companion object {
        val Translate = StampedPathEffectStyle("translate")
        val Rotate = StampedPathEffectStyle("rotate")
        val Morph = StampedPathEffectStyle("morph")
    }
}

interface PathEffect {
    companion object {
        fun dashPathEffect(intervals: FloatArray, phase: Float = 0f): PathEffect {
            require(intervals.size >= 2 && intervals.size % 2 == 0 && intervals.all { it.isFinite() && it >= 0f })
            require(intervals.any { it > 0f } && phase.isFinite())
            val dash = intervals.joinToString(prefix = "[", postfix = "]") { jsonNum(it) }
            return PathEffectSpec("""{"kind":"dash","intervals":$dash,"phase":${jsonNum(phase)}}""")
        }

        fun cornerPathEffect(radius: Float): PathEffect {
            require(radius.isFinite() && radius >= 0f)
            return PathEffectSpec("""{"kind":"corner","radius":${jsonNum(radius)}}""")
        }

        fun chainPathEffect(outer: PathEffect, inner: PathEffect): PathEffect =
            PathEffectSpec("""{"kind":"chain","outer":${outer.spec()},"inner":${inner.spec()}}""")

        fun stampedPathEffect(
            shape: Path,
            advance: Float,
            phase: Float,
            style: StampedPathEffectStyle,
        ): PathEffect {
            require(advance.isFinite() && advance > 0f && phase.isFinite())
            return PathEffectSpec(
                """{"kind":"stamp","shape":${shape.wire(1f)},"advance":${jsonNum(advance)},"phase":${jsonNum(phase)},"style":"${style.wire}"}""",
            )
        }
    }
}

private data class PathEffectSpec(val json: String) : PathEffect
private fun PathEffect.spec(): String = (this as PathEffectSpec).json
internal fun PathEffect.wire(): String = """"pathEffect":${spec()}"""
