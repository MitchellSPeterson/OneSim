package androidx.compose.foundation.text.modifiers

import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.sp

sealed interface TextAutoSizeLayoutScope : Density {
    fun performLayout(constraints: Constraints, text: AnnotatedString, fontSize: TextUnit): TextLayoutResult
}

internal data class TextProbe(
    val constraints: Constraints,
    val text: AnnotatedString,
    val fontSize: TextUnit,
)

internal class TextProbeRequired(val probe: TextProbe) : RuntimeException()

internal class MeasuredTextAutoSizeScope(
    private val measured: Map<TextProbe, TextLayoutResult>,
    private val screen: Density,
    private val baseFontSize: Float,
) : TextAutoSizeLayoutScope, Density by screen {
    override fun TextUnit.toPx(): Float =
        if (isEm) value * baseFontSize * density * fontScale else value * density * fontScale

    override fun performLayout(constraints: Constraints, text: AnnotatedString, fontSize: TextUnit): TextLayoutResult {
        val size = if (fontSize.isEm) (fontSize.value * baseFontSize).sp else fontSize
        require(size.isSp && size.value.isFinite() && size.value >= 0f) { "Expected a nonnegative font size" }
        val probe = TextProbe(constraints, text, size)
        return measured[probe] ?: throw TextProbeRequired(probe)
    }
}
