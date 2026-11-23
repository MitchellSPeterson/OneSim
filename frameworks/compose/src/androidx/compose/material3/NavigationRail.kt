package androidx.compose.material3

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
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
 * Material 3 navigation rail: a full-height column, at least 80 dp wide, of [NavigationRailItem]s 4 dp apart under an
 * optional [header] (a FAB or menu button, 8 dp above the items).
 */
@Composable
fun NavigationRail(
    modifier: Modifier = Modifier,
    containerColor: Color = NavigationRailDefaults.ContainerColor,
    contentColor: Color = contentColorFor(containerColor),
    header: @Composable (ColumnScope.() -> Unit)? = null,
    windowInsets: WindowInsets = NavigationRailDefaults.windowInsets,
    content: @Composable ColumnScope.() -> Unit,
) {
    Surface(color = containerColor, contentColor = contentColor, modifier = modifier) {
        Column(
            Modifier.fillMaxHeight()
                .windowInsetsPadding(windowInsets)
                .widthIn(min = NavigationRailItemWidth)
                .padding(vertical = NavigationRailVerticalPadding)
                .selectableGroup(),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(NavigationRailVerticalPadding),
        ) {
            if (header != null) {
                header()
                Spacer(Modifier.height(NavigationRailHeaderPadding))
            }
            content()
        }
    }
}

/**
 * An item of a [NavigationRail], 80 dp wide and at least 56 dp tall: [icon] on a 56 x 32 dp pill with [label] 4 dp
 * below it, or on a 56 dp circle without a label. Colors and the pill animate as in [NavigationBarItem].
 */
@Composable
fun NavigationRailItem(
    selected: Boolean,
    onClick: () -> Unit,
    icon: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    label: @Composable (() -> Unit)? = null,
    alwaysShowLabel: Boolean = true,
    colors: NavigationRailItemColors = NavigationRailItemDefaults.colors(),
    interactionSource: MutableInteractionSource? = null,
) {
    @Suppress("NAME_SHADOWING")
    val interactionSource = interactionSource ?: remember { MutableInteractionSource() }
    val iconColor by animateColorAsState(colors.iconColor(selected, enabled), NavMotion.defaultEffects())
    val textColor by animateColorAsState(colors.textColor(selected, enabled), NavMotion.defaultEffects())
    val alphaProgress by animateFloatAsState(if (selected) 1f else 0f, NavMotion.defaultEffects())
    val sizeProgress by animateFloatAsState(if (selected) 1f else 0f, NavMotion.fastSpatial())
    val offsetSource = remember(interactionSource) {
        NavMappedInteractionSource(interactionSource) {
            Offset((NavigationRailItemWidth.navRoundToPx() - IndicatorWidth.navRoundToPx()) / 2f, 0f)
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
            .defaultMinSize(minHeight = NavigationRailItemHeight)
            .widthIn(min = NavigationRailItemWidth),
        contentAlignment = Alignment.Center,
    ) {
        val drop = if (label != null && !alwaysShowLabel) LabelIconDistance * (1f - p) else 0.dp
        var body: Modifier = if (label != null) Modifier.padding(bottom = IndicatorVerticalPaddingWithLabel) else Modifier
        if (drop.value != 0f) body = body.offset(y = drop)
        NavTopIconBody(
            modifier = body,
            indicatorWidth = IndicatorWidth,
            indicatorHeight = if (label == null) IndicatorWidth else IndicatorHeightWithLabel,
            indicatorShape = CircleShape,
            indicatorColor = colors.indicatorColor,
            widthProgress = p,
            indicatorAlpha = alphaProgress,
            indicatorFromStart = false,
            rippleSource = offsetSource,
            icon = styledIcon,
            label = styledLabel,
            labelGap = NavigationRailItemVerticalPadding,
            labelAlpha = if (alwaysShowLabel) 1f else alphaProgress,
        )
    }
}

object NavigationRailDefaults {
    /** Default container color of a navigation rail (`surface`). */
    val ContainerColor: Color @Composable get() = MaterialTheme.colorScheme.surface

    /** Default window insets: the system bars at the top and bottom. */
    val windowInsets: WindowInsets
        @Composable get() = WindowInsets.systemBars.only(WindowInsetsSides.Vertical + WindowInsetsSides.Start)
}

object NavigationRailItemDefaults {
    /** The default [NavigationRailItemColors]. */
    @Composable
    fun colors(): NavigationRailItemColors = MaterialTheme.colorScheme.defaultNavigationRailItemColors()

    /** [NavigationRailItemColors] with the given colors. */
    @Composable
    fun colors(
        selectedIconColor: Color = MaterialTheme.colorScheme.onSecondaryContainer,
        selectedTextColor: Color = MaterialTheme.colorScheme.secondary,
        indicatorColor: Color = MaterialTheme.colorScheme.secondaryContainer,
        unselectedIconColor: Color = MaterialTheme.colorScheme.onSurfaceVariant,
        unselectedTextColor: Color = MaterialTheme.colorScheme.onSurfaceVariant,
        disabledIconColor: Color = unselectedIconColor.copy(alpha = NavDisabledAlpha),
        disabledTextColor: Color = unselectedTextColor.copy(alpha = NavDisabledAlpha),
    ): NavigationRailItemColors = MaterialTheme.colorScheme.defaultNavigationRailItemColors().copy(
        selectedIconColor = selectedIconColor,
        selectedTextColor = selectedTextColor,
        selectedIndicatorColor = indicatorColor,
        unselectedIconColor = unselectedIconColor,
        unselectedTextColor = unselectedTextColor,
        disabledIconColor = disabledIconColor,
        disabledTextColor = disabledTextColor,
    )

    internal fun ColorScheme.defaultNavigationRailItemColors(): NavigationRailItemColors = NavigationRailItemColors(
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
    ): NavigationRailItemColors = NavigationRailItemColors(
        selectedIconColor = selectedIconColor,
        selectedTextColor = selectedTextColor,
        selectedIndicatorColor = indicatorColor,
        unselectedIconColor = unselectedIconColor,
        unselectedTextColor = unselectedTextColor,
        disabledIconColor = unselectedIconColor.copy(alpha = NavDisabledAlpha),
        disabledTextColor = unselectedTextColor.copy(alpha = NavDisabledAlpha),
    )
}

/** Colors of a [NavigationRailItem] in its states. */
@Immutable
class NavigationRailItemColors(
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
    ): NavigationRailItemColors = NavigationRailItemColors(
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
        if (other !is NavigationRailItemColors) return false
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

internal val NavigationRailVerticalPadding: Dp = 4.dp
private val NavigationRailHeaderPadding: Dp = 8.dp
/** `NavigationRailCollapsedTokens.NarrowContainerWidth`. */
internal val NavigationRailItemWidth: Dp = 80.dp
/** `NavigationRailVerticalItemTokens.ActiveIndicatorWidth`. */
internal val NavigationRailItemHeight: Dp = 56.dp
internal val NavigationRailItemVerticalPadding: Dp = 4.dp
private val IndicatorWidth: Dp = 56.dp
private val IndicatorHeightWithLabel: Dp = 32.dp
private val IndicatorVerticalPaddingWithLabel: Dp = 4.dp
private val LabelIconDistance: Dp = 12.dp
