package androidx.compose.material3

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.wire
import androidx.compose.ui.jsonNum

/** `minimumInteractiveComponentSize`: a 48 dp touch target around smaller controls. */
fun Modifier.minimumInteractiveComponentSize(): Modifier = wire("""{"minInteractive":${jsonNum(48f)}}""")

// ponytail: tonalElevation does not tint the container (M3 1.2+ surfaces use surfaceContainer* roles instead).
private fun Modifier.surface(shape: Shape, color: Color, border: BorderStroke?, shadowElevation: Dp): Modifier {
    var m = this
    if (shadowElevation > 0.dp) m = m.shadow(shadowElevation, shape, clip = false)
    m = m.background(color, shape)
    if (border != null) m = m.border(border, shape)
    return m.clip(shape)
}

@Composable
fun Surface(
    modifier: Modifier = Modifier,
    shape: Shape = RectangleShape,
    color: Color = MaterialTheme.colorScheme.surface,
    contentColor: Color = contentColorFor(color),
    tonalElevation: Dp = 0.dp,
    shadowElevation: Dp = 0.dp,
    border: BorderStroke? = null,
    content: @Composable () -> Unit,
) {
    CompositionLocalProvider(LocalContentColor provides contentColor) {
        Box(modifier.surface(shape, color, border, shadowElevation), propagateMinConstraints = true) { content() }
    }
}

@Composable
fun Surface(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    shape: Shape = RectangleShape,
    color: Color = MaterialTheme.colorScheme.surface,
    contentColor: Color = contentColorFor(color),
    tonalElevation: Dp = 0.dp,
    shadowElevation: Dp = 0.dp,
    border: BorderStroke? = null,
    interactionSource: Any? = null,
    content: @Composable () -> Unit,
) {
    CompositionLocalProvider(LocalContentColor provides contentColor) {
        Box(
            modifier.minimumInteractiveComponentSize().surface(shape, color, border, shadowElevation)
                .clickable(enabled = enabled, role = Role.Button, onClick = onClick),
            propagateMinConstraints = true,
        ) { content() }
    }
}

@Composable
fun Surface(
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    shape: Shape = RectangleShape,
    color: Color = MaterialTheme.colorScheme.surface,
    contentColor: Color = contentColorFor(color),
    tonalElevation: Dp = 0.dp,
    shadowElevation: Dp = 0.dp,
    border: BorderStroke? = null,
    interactionSource: Any? = null,
    content: @Composable () -> Unit,
) {
    CompositionLocalProvider(LocalContentColor provides contentColor) {
        Box(
            modifier.minimumInteractiveComponentSize().surface(shape, color, border, shadowElevation)
                .selectable(selected = selected, enabled = enabled, onClick = onClick),
            propagateMinConstraints = true,
        ) { content() }
    }
}

@Composable
fun Surface(
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    shape: Shape = RectangleShape,
    color: Color = MaterialTheme.colorScheme.surface,
    contentColor: Color = contentColorFor(color),
    tonalElevation: Dp = 0.dp,
    shadowElevation: Dp = 0.dp,
    border: BorderStroke? = null,
    interactionSource: Any? = null,
    content: @Composable () -> Unit,
) {
    CompositionLocalProvider(LocalContentColor provides contentColor) {
        Box(
            modifier.minimumInteractiveComponentSize().surface(shape, color, border, shadowElevation)
                .toggleable(value = checked, enabled = enabled, onValueChange = onCheckedChange),
            propagateMinConstraints = true,
        ) { content() }
    }
}
