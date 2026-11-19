package androidx.compose.foundation.text

import androidx.compose.foundation.wire
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.jsonNum
import androidx.compose.ui.jsonString
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import simcore.compose.Primitive

/** Text primitive (protocol section K). Font size defaults to 14 sp, color to black, as in Compose. */
@Composable
fun BasicText(
    text: String,
    modifier: Modifier = Modifier,
    style: TextStyle = TextStyle.Default,
    onTextLayout: ((Any) -> Unit)? = null,
    overflow: TextOverflow = TextOverflow.Clip,
    softWrap: Boolean = true,
    maxLines: Int = Int.MAX_VALUE,
    minLines: Int = 1,
    color: (() -> Color)? = null,
) {
    val c = color?.invoke()?.takeIf { it.isSpecified } ?: style.color.takeIf { it.isSpecified } ?: Color.Black
    val p = buildString {
        append(""""text":""").append(jsonString(text))
        append(""","color":""").append(c.wire())
        append(""","fontSize":""").append(jsonNum(if (style.fontSize.isSpecified) style.fontSize.value else 14f))
        if (style.lineHeight.isSpecified) append(""","lineHeight":""").append(jsonNum(lineHeight(style)))
        append(""","fontWeight":""").append((style.fontWeight ?: FontWeight.Normal).weight)
        if (style.letterSpacing.isSpecified) {
            val ls = if (style.letterSpacing.isEm) style.letterSpacing.value * (if (style.fontSize.isSpecified) style.fontSize.value else 14f) else style.letterSpacing.value
            append(""","letterSpacing":""").append(jsonNum(ls))
        }
        when (style.textAlign) {
            TextAlign.Center -> append(""","textAlign":"center"""")
            TextAlign.End, TextAlign.Right -> append(""","textAlign":"end"""")
            else -> {}
        }
        if (maxLines != Int.MAX_VALUE) append(""","maxLines":""").append(maxLines)
        if (!softWrap) append(""","softWrap":false""")
        if (style.textDecoration == TextDecoration.LineThrough) append(""","strike":true""")
    }
    Primitive("Text", modifier, p)
}

private fun lineHeight(style: TextStyle): Float =
    if (style.lineHeight.isEm) style.lineHeight.value * (if (style.fontSize.isSpecified) style.fontSize.value else 14f) else style.lineHeight.value
