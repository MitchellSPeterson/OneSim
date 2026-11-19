package androidx.compose.material3

import androidx.compose.foundation.text.EditableText
import androidx.compose.foundation.wire
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.jsonNum
import androidx.compose.ui.text.TextStyle
import onesim.compose.InputPrimitive
import kotlin.math.roundToInt

@Composable
fun TextField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    readOnly: Boolean = false,
    textStyle: TextStyle = LocalTextStyle.current,
    singleLine: Boolean = false,
) {
    EditableText(value, onValueChange, modifier, enabled && !readOnly, textStyle, singleLine, plain = false)
}

@Composable
fun Slider(
    value: Float,
    onValueChange: (Float) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    valueRange: ClosedFloatingPointRange<Float> = 0f..1f,
    steps: Int = 0,
) {
    require(steps >= 0)
    val (min, max) = valueRange.start to valueRange.endInclusive
    require(min <= max)
    val colors = MaterialTheme.colorScheme
    val props = buildString {
        append("\"value\":").append(jsonNum(value.coerceIn(min, max)))
        append(",\"min\":").append(jsonNum(min))
        append(",\"max\":").append(jsonNum(max))
        append(",\"enabled\":").append(enabled)
        append(",\"activeColor\":").append(colors.primary.wire())
        append(",\"inactiveColor\":").append(colors.secondaryContainer.wire())
    }
    InputPrimitive("Slider", modifier, props, if (enabled && min < max) { input ->
        input.toFloatOrNull()?.takeIf { it.isFinite() }?.coerceIn(min, max)?.let { next ->
            val scale = (max - min) / (steps + 1)
            onValueChange(if (steps == 0) next else (min + ((next - min) / scale).roundToInt() * scale).coerceIn(min, max))
        }
    } else null)
}
