package androidx.compose.material3

import kotlin.jvm.JvmInline
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.indication
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

// ---- Cards -------------------------------------------------------------------------------------------------------

@Immutable
class CardColors(val containerColor: Color, val contentColor: Color, val disabledContainerColor: Color, val disabledContentColor: Color)

@Immutable
class CardElevation internal constructor(internal val elevation: Dp)

object CardDefaults {
    val shape: Shape @Composable get() = MaterialTheme.shapes.medium
    val elevatedShape: Shape @Composable get() = MaterialTheme.shapes.medium
    val outlinedShape: Shape @Composable get() = MaterialTheme.shapes.medium

    @Composable
    fun cardColors(containerColor: Color = Color.Unspecified, contentColor: Color = Color.Unspecified): CardColors {
        val c = containerColor.takeIf { it.isSpecified } ?: MaterialTheme.colorScheme.surfaceContainerHighest
        val fg = contentColor.takeIf { it.isSpecified } ?: contentColorFor(c)
        return CardColors(c, fg, c, fg.copy(alpha = 0.38f))
    }

    @Composable
    fun elevatedCardColors(containerColor: Color = Color.Unspecified, contentColor: Color = Color.Unspecified) =
        cardColors(containerColor.takeIf { it.isSpecified } ?: MaterialTheme.colorScheme.surfaceContainerLow, contentColor)

    @Composable
    fun outlinedCardColors(containerColor: Color = Color.Unspecified, contentColor: Color = Color.Unspecified) =
        cardColors(containerColor.takeIf { it.isSpecified } ?: MaterialTheme.colorScheme.surface, contentColor)

    @Composable
    fun cardElevation(defaultElevation: Dp = 0.dp, pressedElevation: Dp = 0.dp, focusedElevation: Dp = 0.dp, hoveredElevation: Dp = 1.dp, draggedElevation: Dp = 6.dp, disabledElevation: Dp = 0.dp) =
        CardElevation(defaultElevation)

    @Composable
    fun elevatedCardElevation(defaultElevation: Dp = 1.dp, pressedElevation: Dp = 1.dp, focusedElevation: Dp = 1.dp, hoveredElevation: Dp = 2.dp, draggedElevation: Dp = 6.dp, disabledElevation: Dp = 1.dp) =
        CardElevation(defaultElevation)

    @Composable
    fun outlinedCardBorder(enabled: Boolean = true) = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)
}

@Composable
fun Card(
    modifier: Modifier = Modifier,
    shape: Shape = CardDefaults.shape,
    colors: CardColors = CardDefaults.cardColors(),
    elevation: CardElevation = CardDefaults.cardElevation(),
    border: BorderStroke? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    Surface(modifier, shape, colors.containerColor, colors.contentColor, shadowElevation = elevation.elevation, border = border) {
        Column(content = content)
    }
}

@Composable
fun Card(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    shape: Shape = CardDefaults.shape,
    colors: CardColors = CardDefaults.cardColors(),
    elevation: CardElevation = CardDefaults.cardElevation(),
    border: BorderStroke? = null,
    interactionSource: MutableInteractionSource? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    Surface(onClick, modifier, enabled, shape, colors.containerColor, colors.contentColor, shadowElevation = elevation.elevation, border = border, interactionSource = interactionSource) {
        Column(content = content)
    }
}

@Composable
fun ElevatedCard(
    modifier: Modifier = Modifier,
    shape: Shape = CardDefaults.elevatedShape,
    colors: CardColors = CardDefaults.elevatedCardColors(),
    elevation: CardElevation = CardDefaults.elevatedCardElevation(),
    content: @Composable ColumnScope.() -> Unit,
) = Card(modifier, shape, colors, elevation, null, content)

@Composable
fun OutlinedCard(
    modifier: Modifier = Modifier,
    shape: Shape = CardDefaults.outlinedShape,
    colors: CardColors = CardDefaults.outlinedCardColors(),
    elevation: CardElevation = CardDefaults.cardElevation(),
    border: BorderStroke = CardDefaults.outlinedCardBorder(),
    content: @Composable ColumnScope.() -> Unit,
) = Card(modifier, shape, colors, elevation, border, content)

// ---- Dividers ----------------------------------------------------------------------------------------------------

object DividerDefaults {
    val Thickness = 1.dp
    val color: Color @Composable get() = MaterialTheme.colorScheme.outlineVariant
}

@Composable
fun HorizontalDivider(modifier: Modifier = Modifier, thickness: Dp = DividerDefaults.Thickness, color: Color = DividerDefaults.color) {
    Box(modifier.fillMaxWidth().height(thickness).background(color))
}

@Composable
fun VerticalDivider(modifier: Modifier = Modifier, thickness: Dp = DividerDefaults.Thickness, color: Color = DividerDefaults.color) {
    Box(modifier.fillMaxHeight().width(thickness).background(color))
}

// ---- Switch ------------------------------------------------------------------------------------------------------

@Immutable
class SwitchColors(
    val checkedThumbColor: Color, val checkedTrackColor: Color, val checkedBorderColor: Color,
    val uncheckedThumbColor: Color, val uncheckedTrackColor: Color, val uncheckedBorderColor: Color,
)

object SwitchDefaults {
    @Composable
    fun colors(
        checkedThumbColor: Color = MaterialTheme.colorScheme.onPrimary,
        checkedTrackColor: Color = MaterialTheme.colorScheme.primary,
        checkedBorderColor: Color = Color.Transparent,
        uncheckedThumbColor: Color = MaterialTheme.colorScheme.outline,
        uncheckedTrackColor: Color = MaterialTheme.colorScheme.surfaceContainerHighest,
        uncheckedBorderColor: Color = MaterialTheme.colorScheme.outline,
    ) = SwitchColors(checkedThumbColor, checkedTrackColor, checkedBorderColor, uncheckedThumbColor, uncheckedTrackColor, uncheckedBorderColor)
}

/**
 * Material 3 switch: 52 x 32 dp track, 16 dp thumb off / 24 dp on, 28 dp while pressed (no thumb icon), with an
 * unbounded 20 dp ripple on the thumb.
 */
@Composable
fun Switch(
    checked: Boolean,
    onCheckedChange: ((Boolean) -> Unit)?,
    modifier: Modifier = Modifier,
    thumbContent: (@Composable () -> Unit)? = null,
    enabled: Boolean = true,
    colors: SwitchColors = SwitchDefaults.colors(),
    interactionSource: MutableInteractionSource? = null,
) {
    val source = interactionSource ?: remember { MutableInteractionSource() }
    val toggle = if (onCheckedChange != null) {
        Modifier.minimumInteractiveComponentSize().toggleable(checked, source, null, enabled, Role.Switch, onCheckedChange)
    } else {
        Modifier
    }
    val pressed by source.collectIsPressedAsState()
    val track = if (checked) colors.checkedTrackColor else colors.uncheckedTrackColor
    val border = if (checked) colors.checkedBorderColor else colors.uncheckedBorderColor
    val thumb = if (checked) colors.checkedThumbColor else colors.uncheckedThumbColor
    val alpha = if (enabled) 1f else 0.38f
    val d = if (pressed) 28.dp else if (checked) 24.dp else 16.dp
    // Thumb centre: 16 dp from the start (off) or the end (on) of the 52 dp track, whatever its size.
    val x = if (checked) 52.dp - 16.dp - d / 2 else 16.dp - d / 2
    Box(
        modifier.then(toggle).size(52.dp, 32.dp).background(track.copy(alpha = track.alpha * alpha), CircleShape)
            .then(if (border.alpha > 0f) Modifier.border(2.dp, border.copy(alpha = border.alpha * alpha), CircleShape) else Modifier),
        contentAlignment = Alignment.CenterStart,
    ) {
        Box(
            Modifier.offset(x = x).size(d).indication(source, ripple(bounded = false, radius = 20.dp))
                .background(thumb.copy(alpha = thumb.alpha * alpha), CircleShape),
        )
    }
}

// ---- Floating action button ----------------------------------------------------------------------------------------

object FloatingActionButtonDefaults {
    val shape: Shape @Composable get() = MaterialTheme.shapes.large
    val containerColor: Color @Composable get() = MaterialTheme.colorScheme.primaryContainer

    @Composable
    fun elevation(defaultElevation: Dp = 6.dp, pressedElevation: Dp = 6.dp, focusedElevation: Dp = 6.dp, hoveredElevation: Dp = 8.dp) =
        FloatingActionButtonElevation(defaultElevation)
}

@Immutable
class FloatingActionButtonElevation internal constructor(internal val elevation: Dp)

@Composable
fun FloatingActionButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    shape: Shape = FloatingActionButtonDefaults.shape,
    containerColor: Color = FloatingActionButtonDefaults.containerColor,
    contentColor: Color = contentColorFor(containerColor),
    elevation: FloatingActionButtonElevation = FloatingActionButtonDefaults.elevation(),
    interactionSource: MutableInteractionSource? = null,
    content: @Composable () -> Unit,
) {
    Surface(onClick, modifier, true, shape, containerColor, contentColor, shadowElevation = elevation.elevation, interactionSource = interactionSource) {
        ProvideTextStyle(MaterialTheme.typography.labelLarge) {
            Box(Modifier.defaultMinSize(56.dp, 56.dp), contentAlignment = Alignment.Center) { content() }
        }
    }
}

@Composable
fun ExtendedFloatingActionButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    shape: Shape = FloatingActionButtonDefaults.shape,
    containerColor: Color = FloatingActionButtonDefaults.containerColor,
    contentColor: Color = contentColorFor(containerColor),
    elevation: FloatingActionButtonElevation = FloatingActionButtonDefaults.elevation(),
    interactionSource: MutableInteractionSource? = null,
    content: @Composable RowScope.() -> Unit,
) {
    Surface(onClick, modifier, true, shape, containerColor, contentColor, shadowElevation = elevation.elevation, interactionSource = interactionSource) {
        ProvideTextStyle(MaterialTheme.typography.labelLarge) {
            Row(
                Modifier.defaultMinSize(80.dp, 56.dp).padding(horizontal = 20.dp), // text-only extended FAB (material3 1.4.0)
                horizontalArrangement = Arrangement.Center,
                verticalAlignment = Alignment.CenterVertically,
                content = content,
            )
        }
    }
}

// ---- Scaffold ----------------------------------------------------------------------------------------------------------

object ScaffoldDefaults {
    val contentWindowInsets: WindowInsets @Composable get() = WindowInsets.systemBars
}

@JvmInline
value class FabPosition internal constructor(internal val value: Int) {
    companion object {
        val Start = FabPosition(0)
        val Center = FabPosition(1)
        val End = FabPosition(2)
        val EndOverlay = FabPosition(3)
    }
}

/**
 * Bars are stacked around the content instead of drawn over it: the content gets the space between them, and
 * `innerPadding` carries the system-bar insets on edges without a bar.
 */
// ponytail: content does not scroll under the top/bottom bars (real Scaffold overlaps them and pads by their height).
@Composable
fun Scaffold(
    modifier: Modifier = Modifier,
    topBar: @Composable () -> Unit = NoTopBar,
    bottomBar: @Composable () -> Unit = NoBottomBar,
    snackbarHost: @Composable () -> Unit = {},
    floatingActionButton: @Composable () -> Unit = {},
    floatingActionButtonPosition: FabPosition = FabPosition.End,
    containerColor: Color = MaterialTheme.colorScheme.background,
    contentColor: Color = contentColorFor(containerColor),
    contentWindowInsets: WindowInsets = ScaffoldDefaults.contentWindowInsets,
    content: @Composable (PaddingValues) -> Unit,
) {
    val hasTop = topBar !== NoTopBar
    val hasBottom = bottomBar !== NoBottomBar
    val inner = contentWindowInsets.asPaddingValues()
    val padding = PaddingValues(
        top = if (hasTop) 0.dp else inner.calculateTopPadding(),
        bottom = if (hasBottom) 0.dp else inner.calculateBottomPadding(),
    )
    Surface(modifier.fillMaxSize(), color = containerColor, contentColor = contentColor) {
        Column(Modifier.fillMaxSize()) {
            if (hasTop) topBar()
            Box(Modifier.weight(1f).fillMaxWidth()) { content(padding) }
            if (hasBottom) bottomBar()
        }
        val fabAlign = when (floatingActionButtonPosition) {
            FabPosition.Start -> Alignment.BottomStart
            FabPosition.Center -> Alignment.BottomCenter
            else -> Alignment.BottomEnd
        }
        Box(Modifier.fillMaxSize(), contentAlignment = fabAlign) {
            Box(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 16.dp + if (hasBottom) 0.dp else inner.calculateBottomPadding())) {
                floatingActionButton()
            }
        }
    }
}

/** Scaffold's empty slot defaults; a slot the app did not pass is one of these (compared by identity). */
private val NoTopBar: @Composable () -> Unit = {}
private val NoBottomBar: @Composable () -> Unit = {}
