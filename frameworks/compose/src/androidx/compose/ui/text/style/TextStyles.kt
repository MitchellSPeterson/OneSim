package androidx.compose.ui.text.style

import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.sp
import kotlin.jvm.JvmInline

@JvmInline
value class TextAlign(val value: Int) {
    companion object {
        val Left = TextAlign(1)
        val Right = TextAlign(2)
        val Center = TextAlign(3)
        val Justify = TextAlign(4)
        val Start = TextAlign(5)
        val End = TextAlign(6)
        val Unspecified = TextAlign(0)
    }
}

@JvmInline
value class TextOverflow(val value: Int) {
    companion object {
        val Clip = TextOverflow(1)
        val Ellipsis = TextOverflow(2)
        val Visible = TextOverflow(3)
        val StartEllipsis = TextOverflow(4)
        val MiddleEllipsis = TextOverflow(5)
    }
}

class TextDecoration private constructor(val mask: Int) {
    operator fun plus(other: TextDecoration): TextDecoration = TextDecoration(mask or other.mask)
    operator fun contains(other: TextDecoration): Boolean = mask and other.mask == other.mask
    override fun equals(other: Any?): Boolean = other is TextDecoration && mask == other.mask
    override fun hashCode(): Int = mask

    companion object {
        val None = TextDecoration(0)
        val Underline = TextDecoration(1)
        val LineThrough = TextDecoration(2)
        fun combine(decorations: List<TextDecoration>): TextDecoration =
            TextDecoration(decorations.fold(0) { mask, item -> mask or item.mask })
    }
}

enum class ResolvedTextDirection { Ltr, Rtl }

@JvmInline
value class BaselineShift(val multiplier: Float) {
    companion object {
        val None = BaselineShift(0f)
        val Superscript = BaselineShift(0.5f)
        val Subscript = BaselineShift(-0.5f)
    }
}

data class TextIndent(
    val firstLine: TextUnit = 0.sp,
    val restLine: TextUnit = 0.sp,
) {
    companion object { val None = TextIndent() }
}

data class TextGeometricTransform(val scaleX: Float = 1f, val skewX: Float = 0f) {
    companion object { val None = TextGeometricTransform() }
}
