package androidx.compose.material3

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

@Immutable
class ButtonColors(
    val containerColor: Color,
    val contentColor: Color,
    val disabledContainerColor: Color,
    val disabledContentColor: Color,
) {
    fun containerColor(enabled: Boolean) = if (enabled) containerColor else disabledContainerColor
    fun contentColor(enabled: Boolean) = if (enabled) contentColor else disabledContentColor
    fun copy(
        containerColor: Color = this.containerColor,
        contentColor: Color = this.contentColor,
        disabledContainerColor: Color = this.disabledContainerColor,
        disabledContentColor: Color = this.disabledContentColor,
    ) = ButtonColors(
        containerColor.takeIf { it.isSpecified } ?: this.containerColor, contentColor.takeIf { it.isSpecified } ?: this.contentColor,
        disabledContainerColor.takeIf { it.isSpecified } ?: this.disabledContainerColor,
        disabledContentColor.takeIf { it.isSpecified } ?: this.disabledContentColor,
    )
}

@Immutable
class ButtonElevation internal constructor(internal val elevation: Dp)

object ButtonDefaults {
    val ContentPadding = PaddingValues(horizontal = 24.dp, vertical = 8.dp)
    val ButtonWithIconContentPadding = PaddingValues(start = 16.dp, top = 8.dp, end = 24.dp, bottom = 8.dp)
    val TextButtonContentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp)
    val MinWidth = 58.dp
    val MinHeight = 40.dp
    val IconSize = 18.dp
    val IconSpacing = 8.dp

    val shape: Shape @Composable get() = CircleShape
    val elevatedShape: Shape @Composable get() = CircleShape
    val filledTonalShape: Shape @Composable get() = CircleShape
    val outlinedShape: Shape @Composable get() = CircleShape
    val textShape: Shape @Composable get() = CircleShape

    // Default colors per material3 1.4.0 (`FilledButtonTokens`, `OutlinedButtonTokens`, ...): disabled content is
    // onSurfaceVariant @ 38% (onSurface for tonal), disabled container onSurface @ 10% (12% for tonal).
    @Composable
    private fun colors(container: Color, content: Color, disabledContainer: Color, disabledContent: Color) =
        ButtonColors(container, content, disabledContainer, disabledContent)

    private val ColorScheme.disabledContainer get() = onSurface.copy(alpha = 0.1f)
    private val ColorScheme.disabledLabel get() = onSurfaceVariant.copy(alpha = 0.38f)

    @Composable
    fun buttonColors(
        containerColor: Color = Color.Unspecified,
        contentColor: Color = Color.Unspecified,
        disabledContainerColor: Color = Color.Unspecified,
        disabledContentColor: Color = Color.Unspecified,
    ) = MaterialTheme.colorScheme.run { colors(primary, onPrimary, disabledContainer, disabledLabel) }
        .copy(containerColor, contentColor, disabledContainerColor, disabledContentColor)

    @Composable
    fun elevatedButtonColors(
        containerColor: Color = Color.Unspecified,
        contentColor: Color = Color.Unspecified,
        disabledContainerColor: Color = Color.Unspecified,
        disabledContentColor: Color = Color.Unspecified,
    ) = MaterialTheme.colorScheme.run { colors(surfaceContainerLow, primary, disabledContainer, disabledLabel) }
        .copy(containerColor, contentColor, disabledContainerColor, disabledContentColor)

    @Composable
    fun filledTonalButtonColors(
        containerColor: Color = Color.Unspecified,
        contentColor: Color = Color.Unspecified,
        disabledContainerColor: Color = Color.Unspecified,
        disabledContentColor: Color = Color.Unspecified,
    ) = MaterialTheme.colorScheme.run {
        colors(secondaryContainer, onSecondaryContainer, onSurface.copy(alpha = 0.12f), onSurface.copy(alpha = 0.38f))
    }.copy(containerColor, contentColor, disabledContainerColor, disabledContentColor)

    @Composable
    fun outlinedButtonColors(
        containerColor: Color = Color.Unspecified,
        contentColor: Color = Color.Unspecified,
        disabledContainerColor: Color = Color.Unspecified,
        disabledContentColor: Color = Color.Unspecified,
    ) = MaterialTheme.colorScheme.run { colors(Color.Transparent, onSurfaceVariant, Color.Transparent, disabledLabel) }
        .copy(containerColor, contentColor, disabledContainerColor, disabledContentColor)

    @Composable
    fun textButtonColors(
        containerColor: Color = Color.Unspecified,
        contentColor: Color = Color.Unspecified,
        disabledContainerColor: Color = Color.Unspecified,
        disabledContentColor: Color = Color.Unspecified,
    ) = MaterialTheme.colorScheme.run { colors(Color.Transparent, primary, Color.Transparent, disabledLabel) }
        .copy(containerColor, contentColor, disabledContainerColor, disabledContentColor)

    @Composable
    fun buttonElevation(defaultElevation: Dp = 0.dp, pressedElevation: Dp = 0.dp, focusedElevation: Dp = 0.dp, hoveredElevation: Dp = 1.dp, disabledElevation: Dp = 0.dp) =
        ButtonElevation(defaultElevation)

    @Composable
    fun elevatedButtonElevation(defaultElevation: Dp = 1.dp, pressedElevation: Dp = 1.dp, focusedElevation: Dp = 1.dp, hoveredElevation: Dp = 3.dp, disabledElevation: Dp = 0.dp) =
        ButtonElevation(defaultElevation)

    @Composable
    fun outlinedButtonBorder(enabled: Boolean = true): BorderStroke =
        MaterialTheme.colorScheme.outlineVariant.let { BorderStroke(1.dp, if (enabled) it else it.copy(alpha = 0.1f)) }
}

@Composable
fun Button(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    shape: Shape = ButtonDefaults.shape,
    colors: ButtonColors = ButtonDefaults.buttonColors(),
    elevation: ButtonElevation? = ButtonDefaults.buttonElevation(),
    border: BorderStroke? = null,
    contentPadding: PaddingValues = ButtonDefaults.ContentPadding,
    interactionSource: Any? = null,
    content: @Composable RowScope.() -> Unit,
) {
    val contentColor = colors.contentColor(enabled)
    Surface(
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        shape = shape,
        color = colors.containerColor(enabled),
        contentColor = contentColor,
        shadowElevation = if (enabled) elevation?.elevation ?: 0.dp else 0.dp,
        border = border,
    ) {
        CompositionLocalProvider(LocalContentColor provides contentColor) {
            ProvideTextStyle(MaterialTheme.typography.labelLarge) {
                Row(
                    Modifier.defaultMinSize(minWidth = ButtonDefaults.MinWidth, minHeight = ButtonDefaults.MinHeight).padding(contentPadding),
                    horizontalArrangement = Arrangement.Center,
                    verticalAlignment = Alignment.CenterVertically,
                    content = content,
                )
            }
        }
    }
}

@Composable
fun ElevatedButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    shape: Shape = ButtonDefaults.elevatedShape,
    colors: ButtonColors = ButtonDefaults.elevatedButtonColors(),
    elevation: ButtonElevation? = ButtonDefaults.elevatedButtonElevation(),
    border: BorderStroke? = null,
    contentPadding: PaddingValues = ButtonDefaults.ContentPadding,
    interactionSource: Any? = null,
    content: @Composable RowScope.() -> Unit,
) = Button(onClick, modifier, enabled, shape, colors, elevation, border, contentPadding, interactionSource, content)

@Composable
fun FilledTonalButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    shape: Shape = ButtonDefaults.filledTonalShape,
    colors: ButtonColors = ButtonDefaults.filledTonalButtonColors(),
    elevation: ButtonElevation? = ButtonDefaults.buttonElevation(),
    border: BorderStroke? = null,
    contentPadding: PaddingValues = ButtonDefaults.ContentPadding,
    interactionSource: Any? = null,
    content: @Composable RowScope.() -> Unit,
) = Button(onClick, modifier, enabled, shape, colors, elevation, border, contentPadding, interactionSource, content)

@Composable
fun OutlinedButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    shape: Shape = ButtonDefaults.outlinedShape,
    colors: ButtonColors = ButtonDefaults.outlinedButtonColors(),
    elevation: ButtonElevation? = null,
    border: BorderStroke? = ButtonDefaults.outlinedButtonBorder(enabled),
    contentPadding: PaddingValues = ButtonDefaults.ContentPadding,
    interactionSource: Any? = null,
    content: @Composable RowScope.() -> Unit,
) = Button(onClick, modifier, enabled, shape, colors, elevation, border, contentPadding, interactionSource, content)

@Composable
fun TextButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    shape: Shape = ButtonDefaults.textShape,
    colors: ButtonColors = ButtonDefaults.textButtonColors(),
    elevation: ButtonElevation? = null,
    border: BorderStroke? = null,
    contentPadding: PaddingValues = ButtonDefaults.TextButtonContentPadding,
    interactionSource: Any? = null,
    content: @Composable RowScope.() -> Unit,
) = Button(onClick, modifier, enabled, shape, colors, elevation, border, contentPadding, interactionSource, content)
