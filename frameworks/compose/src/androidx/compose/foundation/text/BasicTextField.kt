package androidx.compose.foundation.text

import androidx.compose.foundation.wire
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.jsonNum
import androidx.compose.ui.jsonString
import androidx.compose.ui.text.TextStyle
import onesim.compose.InputPrimitive

@Composable
fun BasicTextField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    readOnly: Boolean = false,
    textStyle: TextStyle = TextStyle.Default,
    singleLine: Boolean = false,
) {
    EditableText(value, onValueChange, modifier, enabled && !readOnly, textStyle, singleLine, plain = true)
}

@Composable
internal fun EditableText(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier,
    editable: Boolean,
    style: TextStyle,
    singleLine: Boolean,
    plain: Boolean,
) {
    val props = buildString {
        append("\"text\":").append(jsonString(value))
        append(",\"plain\":").append(plain)
        append(",\"enabled\":").append(editable)
        append(",\"softWrap\":").append(!singleLine)
        if (style.fontSize.isSpecified) append(",\"fontSize\":").append(jsonNum(style.fontSize.value))
        append(",\"color\":").append(style.color.takeIf { it.isSpecified }?.wire() ?: Color.Black.wire())
    }
    InputPrimitive("TextField", modifier, props, onValueChange.takeIf { editable })
}
