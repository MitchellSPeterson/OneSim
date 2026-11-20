package androidx.compose.ui.text

import androidx.compose.ui.unit.TextUnit
import kotlin.jvm.JvmInline

data class Placeholder(val width: TextUnit, val height: TextUnit, val placeholderVerticalAlign: PlaceholderVerticalAlign) {
    init {
        require(width.isSpecified && height.isSpecified)
        require(width.value >= 0f && height.value >= 0f)
    }
}

@JvmInline
value class PlaceholderVerticalAlign internal constructor(internal val wireValue: String) {
    companion object {
        val AboveBaseline = PlaceholderVerticalAlign("aboveBaseline")
        val Top = PlaceholderVerticalAlign("top")
        val Bottom = PlaceholderVerticalAlign("bottom")
        val Center = PlaceholderVerticalAlign("center")
        val TextTop = PlaceholderVerticalAlign("textTop")
        val TextBottom = PlaceholderVerticalAlign("textBottom")
        val TextCenter = PlaceholderVerticalAlign("textCenter")
    }
}
