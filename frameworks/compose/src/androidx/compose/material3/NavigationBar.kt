package androidx.compose.material3

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.takeOrElse
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import onesim.compose.Guest

/**
 * Material 3 navigation bar: a full-width row of [NavigationBarItem]s, 80 dp tall above the navigation bar inset
 * ([windowInsets]), items 8 dp apart.
 */
@Composable
fun NavigationBar(
    modifier: Modifier = Modifier,
    containerColor: Color = NavigationBarDefaults.containerColor,
    contentColor: Color = MaterialTheme.colorScheme.contentColorFor(containerColor),
    tonalElevation: Dp = NavigationBarDefaults.Elevation,
    windowInsets: WindowInsets = NavigationBarDefaults.windowInsets,
    content: @Composable RowScope.() -> Unit,
) {
    Surface(color = containerColor, contentColor = contentColor, tonalElevation = tonalElevation, modifier = modifier) {
        Row(
            modifier = Modifier.fillMaxWidth()
                .windowInsetsPadding(windowInsets)
                .defaultMinSize(minHeight = NavigationBarHeight)
                .selectableGroup(),
            horizontalArrangement = Arrangement.spacedBy(NavigationBarItemHorizontalPadding),
            verticalAlignment = Alignment.CenterVertically,
            content = content,
        )
    }
}

/**
 * An item of a [NavigationBar]: [icon] on a 56 x 32 dp indicator pill and [label] (`labelMedium`) 4 dp below it. The
 * pill grows from the centre and fades in when [selected]; the icon and label colors animate. With
 * [alwaysShowLabel] = false the label shows only when selected and the icon moves to the centre otherwise.
 */
@Composable
fun RowScope.NavigationBarItem(
    selected: Boolean,
    onClick: () -> Unit,
    icon: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    label: @Composable (() -> Unit)? = null,
    alwaysShowLabel: Boolean = true,
    colors: NavigationBarItemColors = NavigationBarItemDefaults.colors(),
    interactionSource: MutableInteractionSource? = null,
) {
    @Suppress("NAME_SHADOWING")
    val interactionSource = interactionSource ?: remember { MutableInteractionSource() }
    val iconColor by animateColorAsState(colors.iconColor(selected, enabled), NavMotion.defaultEffects())
    val textColor by animateColorAsState(colors.textColor(selected, enabled), NavMotion.defaultEffects())
    val alphaProgress by animateFloatAsState(if (selected) 1f else 0f, NavMotion.defaultEffects())
    val sizeProgress by animateFloatAsState(if (selected) 1f else 0f, NavMotion.fastSpatial())
    val itemWidth = remember { IntArray(1) }
    // The entire item is selectable, but only the indicator pill shows the ripple: presses are re-mapped into the
    // indicator's coordinates.
    val offsetSource = remember(interactionSource) {
        NavMappedInteractionSource(interactionSource) {
            Offset((itemWidth[0] - IndicatorWidth.navRoundToPx()) / 2f, IndicatorVerticalOffset.value * Guest.density)
        }
    }
    val styledIcon: @Composable () -> Unit = { CompositionLocalProvider(LocalContentColor provides iconColor, content = icon) }
    val styledLabel: (@Composable () -> Unit)? = label?.let { l ->
        { NavProvideContentColorTextStyle(textColor, MaterialTheme.typography.labelMedium, l) }
    }
    val p = sizeProgress.coerceAtLeast(0f)
    Box(
        modifier
            .selectable(selected = selected, onClick = onClick, enabled = enabled, role = Role.Tab, interactionSource = interactionSource, indication = null)
            .navObserveWidth(itemWidth)
            .defaultMinSize(minHeight = NavigationBarHeight)
            .weight(1f),
        contentAlignment = Alignment.Center,
    ) {
        // Without the label shown the icon sits in the middle; it moves up by (label + 8 dp) / 2 as the item is
        // selected (a one-line 16 dp label: 12 dp).
        val drop = if (label != null && !alwaysShowLabel) LabelIconDistance * (1f - p) else 0.dp
        var body: Modifier = if (label != null) Modifier.padding(bottom = IndicatorVerticalPadding) else Modifier
        if (drop.value != 0f) body = body.offset(y = drop)
        NavTopIconBody(
            modifier = body,
            indicatorWidth = IndicatorWidth,
            indicatorHeight = IndicatorHeight,
            indicatorShape = CircleShape,
            indicatorColor = colors.indicatorColor,
            widthProgress = p,
            indicatorAlpha = alphaProgress,
            indicatorFromStart = false,
            rippleSource = offsetSource,
            icon = styledIcon,
            label = styledLabel,
            labelGap = NavigationBarIndicatorToLabelPadding,
            labelAlpha = if (alwaysShowLabel) 1f else alphaProgress,
        )
    }
}

object NavigationBarDefaults {
    /** Default elevation for a navigation bar (level 0). */
    val Elevation: Dp = 0.dp

    /** Default color for a navigation bar (`surfaceContainer`). */
    val containerColor: Color @Composable get() = MaterialTheme.colorScheme.surfaceContainer

    /** Default window insets: the bottom system bar. */
    val windowInsets: WindowInsets
        @Composable get() = WindowInsets.systemBars.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom)
}

object NavigationBarItemDefaults {
    /** The default [NavigationBarItemColors]. */
    @Composable
    fun colors(): NavigationBarItemColors = MaterialTheme.colorScheme.defaultNavigationBarItemColors()

    /** [NavigationBarItemColors] with the given colors; [Color.Unspecified] keeps the default. */
    @Composable
    fun colors(
        selectedIconColor: Color = Color.Unspecified,
        selectedTextColor: Color = Color.Unspecified,
        indicatorColor: Color = Color.Unspecified,
        unselectedIconColor: Color = Color.Unspecified,
        unselectedTextColor: Color = Color.Unspecified,
        disabledIconColor: Color = Color.Unspecified,
        disabledTextColor: Color = Color.Unspecified,
    ): NavigationBarItemColors = MaterialTheme.colorScheme.defaultNavigationBarItemColors().copy(
        selectedIconColor = selectedIconColor,
        selectedTextColor = selectedTextColor,
        selectedIndicatorColor = indicatorColor,
        unselectedIconColor = unselectedIconColor,
        unselectedTextColor = unselectedTextColor,
        disabledIconColor = disabledIconColor,
        disabledTextColor = disabledTextColor,
    )

    internal fun ColorScheme.defaultNavigationBarItemColors(): NavigationBarItemColors = NavigationBarItemColors(
        selectedIconColor = onSecondaryContainer,
        selectedTextColor = secondary,
        selectedIndicatorColor = secondaryContainer,
        unselectedIconColor = onSurfaceVariant,
        unselectedTextColor = onSurfaceVariant,
        disabledIconColor = onSurfaceVariant.copy(alpha = NavDisabledAlpha),
        disabledTextColor = onSurfaceVariant.copy(alpha = NavDisabledAlpha),
    )

    @Deprecated("Use overload with disabledIconColor and disabledTextColor", level = DeprecationLevel.HIDDEN)
    @Composable
    fun colors(
        selectedIconColor: Color = MaterialTheme.colorScheme.onSecondaryContainer,
        selectedTextColor: Color = MaterialTheme.colorScheme.secondary,
        indicatorColor: Color = MaterialTheme.colorScheme.secondaryContainer,
        unselectedIconColor: Color = MaterialTheme.colorScheme.onSurfaceVariant,
        unselectedTextColor: Color = MaterialTheme.colorScheme.onSurfaceVariant,
    ): NavigationBarItemColors = NavigationBarItemColors(
        selectedIconColor = selectedIconColor,
        selectedTextColor = selectedTextColor,
        selectedIndicatorColor = indicatorColor,
        unselectedIconColor = unselectedIconColor,
        unselectedTextColor = unselectedTextColor,
        disabledIconColor = unselectedIconColor.copy(alpha = NavDisabledAlpha),
        disabledTextColor = unselectedTextColor.copy(alpha = NavDisabledAlpha),
    )
}

/** Colors of a [NavigationBarItem] in its states. */
@Immutable
class NavigationBarItemColors(
    val selectedIconColor: Color,
    val selectedTextColor: Color,
    val selectedIndicatorColor: Color,
    val unselectedIconColor: Color,
    val unselectedTextColor: Color,
    val disabledIconColor: Color,
    val disabledTextColor: Color,
) {
    /** A copy with some colors replaced; [Color.Unspecified] keeps this one's. */
    fun copy(
        selectedIconColor: Color = this.selectedIconColor,
        selectedTextColor: Color = this.selectedTextColor,
        selectedIndicatorColor: Color = this.selectedIndicatorColor,
        unselectedIconColor: Color = this.unselectedIconColor,
        unselectedTextColor: Color = this.unselectedTextColor,
        disabledIconColor: Color = this.disabledIconColor,
        disabledTextColor: Color = this.disabledTextColor,
    ): NavigationBarItemColors = NavigationBarItemColors(
        selectedIconColor.takeOrElse { this.selectedIconColor },
        selectedTextColor.takeOrElse { this.selectedTextColor },
        selectedIndicatorColor.takeOrElse { this.selectedIndicatorColor },
        unselectedIconColor.takeOrElse { this.unselectedIconColor },
        unselectedTextColor.takeOrElse { this.unselectedTextColor },
        disabledIconColor.takeOrElse { this.disabledIconColor },
        disabledTextColor.takeOrElse { this.disabledTextColor },
    )

    @Stable
    internal fun iconColor(selected: Boolean, enabled: Boolean): Color = when {
        !enabled -> disabledIconColor
        selected -> selectedIconColor
        else -> unselectedIconColor
    }

    @Stable
    internal fun textColor(selected: Boolean, enabled: Boolean): Color = when {
        !enabled -> disabledTextColor
        selected -> selectedTextColor
        else -> unselectedTextColor
    }

    internal val indicatorColor: Color get() = selectedIndicatorColor

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is NavigationBarItemColors) return false
        return selectedIconColor == other.selectedIconColor && unselectedIconColor == other.unselectedIconColor &&
            selectedTextColor == other.selectedTextColor && unselectedTextColor == other.unselectedTextColor &&
            selectedIndicatorColor == other.selectedIndicatorColor && disabledIconColor == other.disabledIconColor &&
            disabledTextColor == other.disabledTextColor
    }

    override fun hashCode(): Int {
        var result = selectedIconColor.hashCode()
        result = 31 * result + unselectedIconColor.hashCode()
        result = 31 * result + selectedTextColor.hashCode()
        result = 31 * result + unselectedTextColor.hashCode()
        result = 31 * result + selectedIndicatorColor.hashCode()
        result = 31 * result + disabledIconColor.hashCode()
        result = 31 * result + disabledTextColor.hashCode()
        return result
    }
}

/** `NavigationBarTokens.TallContainerHeight`. */
private val NavigationBarHeight: Dp = 80.dp
internal val NavigationBarItemHorizontalPadding: Dp = 8.dp
internal val NavigationBarIndicatorToLabelPadding: Dp = 4.dp
/** `NavigationBarVerticalItemTokens.ActiveIndicatorWidth` x `ActiveIndicatorHeight`. */
private val IndicatorWidth: Dp = 56.dp
private val IndicatorHeight: Dp = 32.dp
/** (indicator height - 24 dp icon) / 2. */
private val IndicatorVerticalPadding: Dp = 4.dp
private val IndicatorVerticalOffset: Dp = 12.dp
/** How far the icon moves with `alwaysShowLabel = false`: (one 16 dp label line + 8 dp) / 2. */
private val LabelIconDistance: Dp = 12.dp
