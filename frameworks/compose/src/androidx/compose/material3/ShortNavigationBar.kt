package androidx.compose.material3

import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScopeInstance
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.jvm.JvmInline

/**
 * Material 3 short navigation bar: 64 dp tall above the navigation bar inset, its items either sharing the width
 * equally ([ShortNavigationBarArrangement.EqualWeight]) or centred with side margins
 * ([ShortNavigationBarArrangement.Centered]).
 */
@Composable
fun ShortNavigationBar(
    modifier: Modifier = Modifier,
    containerColor: Color = ShortNavigationBarDefaults.containerColor,
    contentColor: Color = ShortNavigationBarDefaults.contentColor,
    windowInsets: WindowInsets = ShortNavigationBarDefaults.windowInsets,
    arrangement: ShortNavigationBarArrangement = ShortNavigationBarDefaults.arrangement,
    content: @Composable () -> Unit,
) {
    require(arrangement == ShortNavigationBarArrangement.EqualWeight || arrangement == ShortNavigationBarArrangement.Centered) {
        "Invalid ItemsArrangement value."
    }
    // The items register themselves so the centred arrangement knows how many there are (AndroidX measures them).
    val items = remember { mutableStateListOf<Any>() }
    val count = items.size
    Surface(color = containerColor, contentColor = contentColor, modifier = modifier) {
        Row(
            Modifier.fillMaxWidth()
                .windowInsetsPadding(windowInsets)
                .defaultMinSize(minHeight = ShortNavigationBarHeight)
                .selectableGroup(),
            horizontalArrangement = if (arrangement == ShortNavigationBarArrangement.Centered) Arrangement.Center else Arrangement.Start,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // Centred: each item gets at most 1/n of the width and at least its share of the middle (1 - 2 x side
            // margin); the group is centred (AndroidX shrinks the margins as wide items grow, which centring matches).
            val side = if (arrangement == ShortNavigationBarArrangement.Centered) centeredSideFraction(count) else 0f
            CompositionLocalProvider(LocalNavShortBarScope provides NavShortBarScope(arrangement, 1f - 2 * side, items)) {
                content()
            }
        }
    }
}

/** AndroidX `calculateCenteredContentHorizontalPadding`: each side's share of the bar width. */
private fun centeredSideFraction(itemsCount: Int): Float {
    if (itemsCount < 1 || itemsCount > 6) return 0f
    return ((100 - 10 * (itemsCount + 3)) / 2f) / 100
}

/** How a [ShortNavigationBar] places its items. */
@JvmInline
value class ShortNavigationBarArrangement private constructor(private val value: Int) {
    companion object {
        /* The items share the bar's width equally. */
        val EqualWeight: ShortNavigationBarArrangement get() = ShortNavigationBarArrangement(0)

        /* The items are centred, with margins that shrink as more items are added. */
        val Centered: ShortNavigationBarArrangement get() = ShortNavigationBarArrangement(1)
    }

    override fun toString(): String = when (this) {
        EqualWeight -> "EqualWeight"
        Centered -> "Centered"
        else -> "Unknown"
    }
}

/**
 * An item of a [ShortNavigationBar]: [icon] above [label] on a 56 x 32 dp pill ([NavigationItemIconPosition.Top]), or
 * icon and label side by side on a 40 dp tall pill ([NavigationItemIconPosition.Start]).
 */
@Composable
fun ShortNavigationBarItem(
    selected: Boolean,
    onClick: () -> Unit,
    icon: @Composable () -> Unit,
    label: @Composable (() -> Unit)?,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    iconPosition: NavigationItemIconPosition = NavigationItemIconPosition.Top,
    colors: NavigationItemColors = ShortNavigationBarItemDefaults.colors(),
    interactionSource: MutableInteractionSource? = null,
) {
    @Suppress("NAME_SHADOWING")
    val interactionSource = interactionSource ?: remember { MutableInteractionSource() }
    val isIconPositionTop = iconPosition == NavigationItemIconPosition.Top
    val bar = LocalNavShortBarScope.current
    if (bar != null) {
        val token = remember { Any() }
        DisposableEffect(bar.items, token) {
            bar.items.add(token)
            onDispose { bar.items.remove(token) }
        }
    }
    // In a bar every item takes its share of the width and the bar's height (AndroidX measures them to fixed sizes).
    val centered = bar?.arrangement == ShortNavigationBarArrangement.Centered
    val itemModifier = if (bar != null) {
        with(RowScopeInstance) { Modifier.weight(1f, fill = !centered) }.defaultMinSize(minHeight = ShortNavigationBarHeight)
    } else {
        Modifier
    }
    NavigationItemImpl(
        selected = selected,
        onClick = onClick,
        icon = icon,
        labelTextStyle = MaterialTheme.typography.labelMedium,
        indicatorShape = CircleShape,
        indicatorWidth = TopIconIndicatorWidth,
        indicatorHorizontalPadding = if (isIconPositionTop) TopIconIndicatorHorizontalPadding else StartIconIndicatorHorizontalPadding,
        indicatorVerticalPadding = if (isIconPositionTop) TopIconIndicatorVerticalPadding else StartIconIndicatorVerticalPadding,
        indicatorToLabelVerticalPadding = TopIconIndicatorToLabelPadding,
        startIconToLabelHorizontalPadding = StartIconToLabelPadding,
        topIconItemVerticalPadding = TopIconItemVerticalPadding,
        textColor = colors.textColor(selected, enabled, isIconPositionTop),
        iconColor = colors.iconColor(selected, enabled),
        indicatorColor = colors.selectedIndicatorColor,
        modifier = modifier,
        enabled = enabled,
        label = label,
        iconPosition = iconPosition,
        interactionSource = interactionSource,
        itemModifier = itemModifier,
        minWidthFraction = if (centered) bar!!.itemWeight else 0f,
    )
}

object ShortNavigationBarDefaults {
    /** Default container color (`surfaceContainer`). */
    val containerColor: Color @Composable get() = MaterialTheme.colorScheme.surfaceContainer

    /** Default content color. */
    val contentColor: Color @Composable get() = contentColorFor(containerColor)

    /** Default items arrangement. */
    val arrangement: ShortNavigationBarArrangement get() = ShortNavigationBarArrangement.EqualWeight

    /** Default window insets: the bottom system bar. */
    val windowInsets: WindowInsets
        @Composable get() = WindowInsets.systemBars.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom)
}

object ShortNavigationBarItemDefaults {
    /** The default [NavigationItemColors] of a [ShortNavigationBarItem]. */
    @Composable
    fun colors(): NavigationItemColors = MaterialTheme.colorScheme.defaultShortNavigationBarItemColors()

    /** [NavigationItemColors] with the given colors. */
    @Composable
    fun colors(
        selectedIconColor: Color = MaterialTheme.colorScheme.onSecondaryContainer,
        selectedTextColorTopIconPosition: Color = MaterialTheme.colorScheme.secondary,
        selectedTextColorStartIconPosition: Color = MaterialTheme.colorScheme.onSecondaryContainer,
        selectedIndicatorColor: Color = MaterialTheme.colorScheme.secondaryContainer,
        unselectedIconColor: Color = MaterialTheme.colorScheme.onSurfaceVariant,
        unselectedTextColor: Color = MaterialTheme.colorScheme.onSurfaceVariant,
        disabledIconColor: Color = unselectedIconColor.copy(alpha = NavDisabledAlpha),
        disabledTextColor: Color = unselectedTextColor.copy(alpha = NavDisabledAlpha),
    ): NavigationItemColors = MaterialTheme.colorScheme.defaultShortNavigationBarItemColors().copy(
        selectedIconColor = selectedIconColor,
        selectedTextColorTopIconPosition = selectedTextColorTopIconPosition,
        selectedTextColorStartIconPosition = selectedTextColorStartIconPosition,
        selectedIndicatorColor = selectedIndicatorColor,
        unselectedIconColor = unselectedIconColor,
        unselectedTextColor = unselectedTextColor,
        disabledIconColor = disabledIconColor,
        disabledTextColor = disabledTextColor,
    )

    @Deprecated("Maintained for binary compatibility", level = DeprecationLevel.HIDDEN)
    @Composable
    fun colors(
        selectedIconColor: Color = MaterialTheme.colorScheme.onSecondaryContainer,
        selectedTextColor: Color = MaterialTheme.colorScheme.secondary,
        selectedIndicatorColor: Color = MaterialTheme.colorScheme.secondaryContainer,
        unselectedIconColor: Color = MaterialTheme.colorScheme.onSurfaceVariant,
        unselectedTextColor: Color = MaterialTheme.colorScheme.onSurfaceVariant,
        disabledIconColor: Color = unselectedIconColor.copy(alpha = NavDisabledAlpha),
        disabledTextColor: Color = unselectedTextColor.copy(alpha = NavDisabledAlpha),
    ): NavigationItemColors = colors(
        selectedIconColor = selectedIconColor,
        selectedTextColorTopIconPosition = selectedTextColor,
        selectedTextColorStartIconPosition = selectedTextColor,
        selectedIndicatorColor = selectedIndicatorColor,
        unselectedIconColor = unselectedIconColor,
        unselectedTextColor = unselectedTextColor,
        disabledIconColor = disabledIconColor,
        disabledTextColor = disabledTextColor,
    )

    internal fun ColorScheme.defaultShortNavigationBarItemColors(): NavigationItemColors = NavigationItemColors(
        selectedIconColor = onSecondaryContainer,
        selectedTextColorTopIconPosition = secondary,
        selectedTextColorStartIconPosition = onSecondaryContainer,
        selectedIndicatorColor = secondaryContainer,
        unselectedIconColor = onSurfaceVariant,
        unselectedTextColor = onSurfaceVariant,
        disabledIconColor = onSurfaceVariant.copy(alpha = NavDisabledAlpha),
        disabledTextColor = onSurfaceVariant.copy(alpha = NavDisabledAlpha),
    )
}

/** `NavigationBarTokens.ContainerHeight`. */
private val ShortNavigationBarHeight: Dp = 64.dp
/** `NavigationBarVerticalItemTokens.ContainerBetweenSpace`. */
private val TopIconItemVerticalPadding: Dp = 6.dp
private val TopIconIndicatorWidth: Dp = 56.dp
/** (32 - 24) / 2. */
private val TopIconIndicatorVerticalPadding: Dp = 4.dp
/** (56 - 24) / 2. */
private val TopIconIndicatorHorizontalPadding: Dp = 16.dp
/** (`NavigationBarHorizontalItemTokens.ActiveIndicatorHeight` 40 - 24) / 2. */
private val StartIconIndicatorVerticalPadding: Dp = 8.dp
private val TopIconIndicatorToLabelPadding: Dp = 4.dp
/** `NavigationBarHorizontalItemTokens.ActiveIndicatorLeadingSpace`. */
private val StartIconIndicatorHorizontalPadding: Dp = 16.dp
/** `NavigationBarTokens.ItemActiveIndicatorIconLabelSpace`. */
private val StartIconToLabelPadding: Dp = 4.dp
