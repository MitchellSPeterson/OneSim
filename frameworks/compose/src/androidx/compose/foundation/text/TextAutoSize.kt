package androidx.compose.foundation.text

import androidx.compose.foundation.text.modifiers.TextAutoSizeLayoutScope
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.sp
import kotlin.math.floor

interface TextAutoSize {
    fun TextAutoSizeLayoutScope.getFontSize(constraints: Constraints, text: AnnotatedString): TextUnit
    override fun equals(other: Any?): Boolean
    override fun hashCode(): Int

    companion object {
        fun StepBased(
            minFontSize: TextUnit = TextAutoSizeDefaults.MinFontSize,
            maxFontSize: TextUnit = TextAutoSizeDefaults.MaxFontSize,
            stepSize: TextUnit = 0.25.sp,
        ): TextAutoSize {
            require(minFontSize.isSpecified && minFontSize.value.isFinite() && minFontSize.value >= 0f)
            require(maxFontSize.isSpecified && maxFontSize.value.isFinite() && maxFontSize.value >= 0f)
            require(stepSize.isSpecified && stepSize.value.isFinite() &&
                stepSize.value >= if (stepSize.isEm) Float.MIN_VALUE else 0.0001f)
            return StepTextAutoSize(minFontSize, maxFontSize, stepSize)
        }
    }
}

object TextAutoSizeDefaults {
    val MinFontSize = 12.sp
    val MaxFontSize = 112.sp
}

internal data class StepTextAutoSize(
    val minimum: TextUnit,
    val maximum: TextUnit,
    val step: TextUnit,
) : TextAutoSize {
    override fun TextAutoSizeLayoutScope.getFontSize(constraints: Constraints, text: AnnotatedString): TextUnit {
        val max = maximum.toPx() / density
        val min = minimum.toPx().div(density).coerceAtMost(max)
        val increment = step.toPx() / density
        var low = 0L
        var high = floor(((max - min) / increment).toDouble()).toLong()
        while (low < high) {
            val difference = high - low
            val middle = low + difference / 2 + difference % 2
            if (performLayout(constraints, text, (min + (middle * increment)).sp).hasVisualOverflow) high = middle - 1
            else low = middle
        }
        return (min + (low * increment)).sp
    }
}
