package androidx.compose.ui.text

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.drawscope.DrawStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDirection
import androidx.compose.ui.text.style.takeOrElse
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextIndent
import androidx.compose.ui.text.style.Hyphens
import androidx.compose.ui.text.style.LineBreak
import androidx.compose.ui.text.intl.LocaleList
import androidx.compose.ui.text.style.TextGeometricTransform
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
    val textIndent: TextIndent? = null,
    val lineBreak: LineBreak = LineBreak.Unspecified,
    val hyphens: Hyphens = Hyphens.Unspecified,
    val fontFeatureSettings: String? = null,
    val brush: Brush? = null,
    val alpha: Float = Float.NaN,
    val shadow: Shadow? = null,
    val drawStyle: DrawStyle? = null,
    val textGeometricTransform: TextGeometricTransform? = null,
    val localeList: LocaleList? = null,
    val textDirection: TextDirection = TextDirection.Unspecified,
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
            textIndent = other.textIndent ?: textIndent,
            lineBreak = if (other.lineBreak != LineBreak.Unspecified) other.lineBreak else lineBreak,
            hyphens = if (other.hyphens != Hyphens.Unspecified) other.hyphens else hyphens,
            fontFeatureSettings = other.fontFeatureSettings ?: fontFeatureSettings,
            brush = other.brush ?: brush.takeUnless { other.color.isSpecified },
            alpha = if (other.alpha.isFinite()) other.alpha else alpha,
            shadow = other.shadow ?: shadow,
            drawStyle = other.drawStyle ?: drawStyle,
            textGeometricTransform = other.textGeometricTransform ?: textGeometricTransform,
            localeList = other.localeList ?: localeList,
            textDirection = other.textDirection.takeOrElse { textDirection },
        )
    }

    operator fun plus(other: TextStyle) = merge(other)

    companion object {
        val Default = TextStyle()
    }
}
