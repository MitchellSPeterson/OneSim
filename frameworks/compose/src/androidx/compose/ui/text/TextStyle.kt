package androidx.compose.ui.text

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.TextUnit

@Immutable
data class TextStyle(
    val color: Color = Color.Unspecified,
    val fontSize: TextUnit = TextUnit.Unspecified,
    val fontWeight: FontWeight? = null,
    val fontStyle: FontStyle? = null,
    val fontFamily: FontFamily? = null,
    val letterSpacing: TextUnit = TextUnit.Unspecified,
    val textDecoration: TextDecoration? = null,
    val textAlign: TextAlign = TextAlign.Unspecified,
    val lineHeight: TextUnit = TextUnit.Unspecified,
) {
    /** `other`'s specified values win. */
    fun merge(other: TextStyle?): TextStyle {
        if (other == null || other == Default) return this
        return TextStyle(
            color = if (other.color.isSpecified) other.color else color,
            fontSize = if (other.fontSize.isSpecified) other.fontSize else fontSize,
            fontWeight = other.fontWeight ?: fontWeight,
            fontStyle = other.fontStyle ?: fontStyle,
            fontFamily = other.fontFamily ?: fontFamily,
            letterSpacing = if (other.letterSpacing.isSpecified) other.letterSpacing else letterSpacing,
            textDecoration = other.textDecoration ?: textDecoration,
            textAlign = if (other.textAlign != TextAlign.Unspecified) other.textAlign else textAlign,
            lineHeight = if (other.lineHeight.isSpecified) other.lineHeight else lineHeight,
        )
    }

    operator fun plus(other: TextStyle) = merge(other)

    companion object {
        val Default = TextStyle()
    }
}
