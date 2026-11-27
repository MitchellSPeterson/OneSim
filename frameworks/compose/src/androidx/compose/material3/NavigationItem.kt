package androidx.compose.material3

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.indication
import androidx.compose.foundation.interaction.Interaction
import androidx.compose.foundation.interaction.InteractionSource
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.selection.selectable
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.takeOrElse
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerInputEventHandler
import androidx.compose.ui.jsonString
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.jvm.JvmInline
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import onesim.compose.Guest
import onesim.compose.NodeElement
import onesim.compose.NodeState
import onesim.compose.PointerTarget
import onesim.compose.WireCx

/** Position of the icon relative to the label in a navigation item. */
@JvmInline
value class NavigationItemIconPosition private constructor(private val value: Int) {
    companion object {
        /* The icon is positioned on top of the label. */
        val Top: NavigationItemIconPosition get() = NavigationItemIconPosition(0)

        /* The icon is positioned at the start of the label. */
        val Start: NavigationItemIconPosition get() = NavigationItemIconPosition(1)
    }

    override fun toString(): String = when (this) {
        Top -> "Top"
        Start -> "Start"
        else -> "Unknown"
    }
}

/** Colors of a [ShortNavigationBarItem] or [WideNavigationRailItem] in its states. */
@Immutable
class NavigationItemColors(
    val selectedIconColor: Color,
    val selectedTextColorTopIconPosition: Color,
    val selectedTextColorStartIconPosition: Color,
    val selectedIndicatorColor: Color,
    val unselectedIconColor: Color,
    val unselectedTextColor: Color,
    val disabledIconColor: Color,
    val disabledTextColor: Color,
) {
    @Deprecated(
        message = "Use the NavigationItemColors constructor that takes selectedTextColorTopIconPosition " +
            "and selectedTextColorStartIconPosition",
        replaceWith = ReplaceWith(
            "NavigationItemColors(selectedIconColor, selectedTextColorTopIconPosition, " +
                "selectedTextColorStartIconPosition, selectedIndicatorColor, " +
                "unselectedIconColor, unselectedTextColor, disabledIconColor, disabledTextColor)",
        ),
        level = DeprecationLevel.WARNING,
    )
    constructor(
        selectedIconColor: Color,
        selectedTextColor: Color,
        selectedIndicatorColor: Color,
        unselectedIconColor: Color,
        unselectedTextColor: Color,
        disabledIconColor: Color,
        disabledTextColor: Color,
    ) : this(
        selectedIconColor, selectedTextColor, selectedTextColor, selectedIndicatorColor, unselectedIconColor,
        unselectedTextColor, disabledIconColor, disabledTextColor,
    )

    @Deprecated(message = "Maintained for binary compatibility", level = DeprecationLevel.HIDDEN)
    fun copy(
        selectedIconColor: Color = this.selectedIconColor,
        selectedTextColor: Color = Color.Unspecified,
        selectedIndicatorColor: Color = this.selectedIndicatorColor,
        unselectedIconColor: Color = this.unselectedIconColor,
        unselectedTextColor: Color = this.unselectedTextColor,
        disabledIconColor: Color = this.disabledIconColor,
        disabledTextColor: Color = this.disabledTextColor,
    ): NavigationItemColors = NavigationItemColors(
        selectedIconColor = selectedIconColor.takeOrElse { this.selectedIconColor },
        selectedTextColorTopIconPosition = selectedTextColor.takeOrElse { this.selectedTextColorTopIconPosition },
        selectedTextColorStartIconPosition = selectedTextColor.takeOrElse { this.selectedTextColorStartIconPosition },
        selectedIndicatorColor = selectedIndicatorColor.takeOrElse { this.selectedIndicatorColor },
        unselectedIconColor = unselectedIconColor.takeOrElse { this.unselectedIconColor },
        unselectedTextColor = unselectedTextColor.takeOrElse { this.unselectedTextColor },
        disabledIconColor = disabledIconColor.takeOrElse { this.disabledIconColor },
        disabledTextColor = disabledTextColor.takeOrElse { this.disabledTextColor },
    )

    /** A copy with some colors replaced; [Color.Unspecified] keeps this one's. */
    fun copy(
        selectedIconColor: Color = this.selectedIconColor,
        selectedTextColorTopIconPosition: Color = this.selectedTextColorTopIconPosition,
        selectedTextColorStartIconPosition: Color = this.selectedTextColorStartIconPosition,
        selectedIndicatorColor: Color = this.selectedIndicatorColor,
        unselectedIconColor: Color = this.unselectedIconColor,
        unselectedTextColor: Color = this.unselectedTextColor,
        disabledIconColor: Color = this.disabledIconColor,
        disabledTextColor: Color = this.disabledTextColor,
    ): NavigationItemColors = NavigationItemColors(
        selectedIconColor = selectedIconColor.takeOrElse { this.selectedIconColor },
        selectedTextColorTopIconPosition = selectedTextColorTopIconPosition.takeOrElse { this.selectedTextColorTopIconPosition },
        selectedTextColorStartIconPosition =
            selectedTextColorStartIconPosition.takeOrElse { this.selectedTextColorStartIconPosition },
        selectedIndicatorColor = selectedIndicatorColor.takeOrElse { this.selectedIndicatorColor },
        unselectedIconColor = unselectedIconColor.takeOrElse { this.unselectedIconColor },
        unselectedTextColor = unselectedTextColor.takeOrElse { this.unselectedTextColor },
        disabledIconColor = disabledIconColor.takeOrElse { this.disabledIconColor },
        disabledTextColor = disabledTextColor.takeOrElse { this.disabledTextColor },
    )

    @Deprecated(
        message = "Use selectedTextColorTopIconPosition instead",
        replaceWith = ReplaceWith("selectedTextColorTopIconPosition"),
        level = DeprecationLevel.WARNING,
    )
    val selectedTextColor: Color get() = selectedTextColorTopIconPosition

    @Stable
    fun iconColor(selected: Boolean, enabled: Boolean): Color = when {
        !enabled -> disabledIconColor
        selected -> selectedIconColor
        else -> unselectedIconColor
    }

    @Deprecated(
        message = "Use the overload that takes isIconPositionTop instead",
        replaceWith = ReplaceWith("textColor(selected, enabled, true)"),
        level = DeprecationLevel.HIDDEN,
    )
    @Stable
    fun textColor(selected: Boolean, enabled: Boolean): Color = textColor(selected = selected, enabled = enabled, isIconPositionTop = true)

    @Stable
    fun textColor(selected: Boolean, enabled: Boolean, isIconPositionTop: Boolean = true): Color = when {
        !enabled -> disabledTextColor
        selected && !isIconPositionTop -> selectedTextColorStartIconPosition
        selected && isIconPositionTop -> selectedTextColorTopIconPosition
        else -> unselectedTextColor
    }

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is NavigationItemColors) return false
        return selectedIconColor == other.selectedIconColor && unselectedIconColor == other.unselectedIconColor &&
            selectedTextColorTopIconPosition == other.selectedTextColorTopIconPosition &&
            selectedTextColorStartIconPosition == other.selectedTextColorStartIconPosition &&
            unselectedTextColor == other.unselectedTextColor && selectedIndicatorColor == other.selectedIndicatorColor &&
            disabledIconColor == other.disabledIconColor && disabledTextColor == other.disabledTextColor
    }

    override fun hashCode(): Int {
        var result = selectedIconColor.hashCode()
        result = 31 * result + unselectedIconColor.hashCode()
        result = 31 * result + selectedTextColorTopIconPosition.hashCode()
        result = 31 * result + selectedTextColorStartIconPosition.hashCode()
        result = 31 * result + unselectedTextColor.hashCode()
        result = 31 * result + selectedIndicatorColor.hashCode()
        result = 31 * result + disabledIconColor.hashCode()
        result = 31 * result + disabledTextColor.hashCode()
        return result
    }
}

// ---- Shared by the navigation bars and rails --------------------------------------------------------------------------

/** Material motion scheme `standard()` springs the navigation components use (`StandardMotionTokens`). */
internal object NavMotion {
    fun <T> defaultSpatial(): FiniteAnimationSpec<T> = spring(dampingRatio = 0.9f, stiffness = 700f)
    fun <T> fastSpatial(): FiniteAnimationSpec<T> = spring(dampingRatio = 0.9f, stiffness = 1400f)
    fun <T> defaultEffects(): FiniteAnimationSpec<T> = spring(dampingRatio = 1f, stiffness = 1600f)
}

/** Material's disabled content alpha. */
internal const val NavDisabledAlpha = 0.38f

/**
 * An [InteractionSource] whose presses are moved by `-delta()` pixels: the whole item is clickable but the ripple is
 * drawn by the indicator-shaped node inside it (AndroidX `MappedInteractionSource`).
 */
internal class NavMappedInteractionSource(source: InteractionSource, private val delta: () -> Offset) : InteractionSource {
    private val mapped = HashMap<PressInteraction.Press, PressInteraction.Press>()

    override val interactions: Flow<Interaction> = source.interactions.map { i ->
        when (i) {
            is PressInteraction.Press -> PressInteraction.Press(i.pressPosition - delta()).also { mapped[i] = it }
            is PressInteraction.Release -> mapped.remove(i.press)?.let { PressInteraction.Release(it) } ?: i
            is PressInteraction.Cancel -> mapped.remove(i.press)?.let { PressInteraction.Cancel(it) } ?: i
            else -> i
        }
    }
}

/**
 * Records the node's width in pixels into [holder] when a touch reaches it (before the clickable's press is emitted), the
 * stand-in for AndroidX's `onSizeChanged` the item uses to place its ripple. It is a passive gesture layer.
 */
internal fun Modifier.navObserveWidth(holder: IntArray): Modifier = this then NavWidthElement(holder)

private class NavWidthElement(val holder: IntArray) : NodeElement {
    override fun create(): NodeState = PointerTarget(
        PointerInputEventHandler {
            awaitPointerEventScope {
                while (true) {
                    awaitPointerEvent(PointerEventPass.Initial)
                    holder[0] = size.width
                }
            }
        },
    )

    override fun wire(state: NodeState, cx: WireCx) {
        cx.add("""{"gesture":{"key":${jsonString(cx.gesture(state as PointerTarget))},"pans":false}}""")
    }

    override fun equals(other: Any?) = other is NavWidthElement && other.holder === holder
    override fun hashCode() = holder.hashCode()
}

@Composable
internal fun NavProvideContentColorTextStyle(contentColor: Color, textStyle: TextStyle, content: @Composable () -> Unit) {
    val merged = LocalTextStyle.current.merge(textStyle)
    CompositionLocalProvider(LocalContentColor provides contentColor, LocalTextStyle provides merged, content = content)
}

internal fun Dp.navRoundToPx(): Float = kotlin.math.round(value * Guest.density)

internal fun navLerp(a: Dp, b: Dp, t: Float): Dp = Dp(a.value + (b.value - a.value) * t)

/**
 * The top-icon item body: the indicator area ([indicatorWidth] x [indicatorHeight], the pill growing to
 * [widthProgress] of its width and fading with [indicatorAlpha]) holding the icon and the indicator-shaped ripple, then
 * the label [labelGap] below it. AndroidX measures the icon; here the indicator has its token size (a 24 dp icon).
 */
@Composable
internal fun NavTopIconBody(
    modifier: Modifier,
    indicatorWidth: Dp,
    indicatorHeight: Dp,
    indicatorShape: Shape,
    indicatorColor: Color,
    widthProgress: Float,
    indicatorAlpha: Float,
    indicatorFromStart: Boolean,
    rippleSource: InteractionSource,
    icon: @Composable () -> Unit,
    label: (@Composable () -> Unit)?,
    labelGap: Dp,
    labelAlpha: Float,
) {
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        Box(
            Modifier.size(indicatorWidth, indicatorHeight),
            contentAlignment = if (indicatorFromStart) Alignment.CenterStart else Alignment.Center,
        ) {
            val w = indicatorWidth * widthProgress.coerceAtLeast(0f)
            if (w.value > 0f && indicatorAlpha > 0f) {
                Box(Modifier.width(w).height(indicatorHeight).alpha(indicatorAlpha.coerceIn(0f, 1f)).background(indicatorColor, indicatorShape))
            }
            Box(Modifier.align(Alignment.Center)) { icon() }
            Box(Modifier.size(indicatorWidth, indicatorHeight).clip(indicatorShape).indication(rippleSource, ripple()))
        }
        if (label != null) {
            Spacer(Modifier.height(labelGap))
            Box(if (labelAlpha < 1f) Modifier.alpha(labelAlpha.coerceIn(0f, 1f)) else Modifier) { label() }
        }
    }
}

/**
 * The start-icon item body: icon and label in a row on the indicator pill (the pill is the row's background, so it fades
 * in instead of growing from the centre), with the ripple over it.
 */
@Composable
internal fun NavStartIconBody(
    modifier: Modifier,
    indicatorShape: Shape,
    indicatorColor: Color,
    indicatorAlpha: Float,
    padding: PaddingValues,
    iconToLabel: Dp,
    rippleSource: InteractionSource,
    icon: @Composable () -> Unit,
    label: @Composable () -> Unit,
    labelAlpha: Float,
) {
    val a = indicatorAlpha.coerceIn(0f, 1f)
    val bg = if (a > 0f) Modifier.background(indicatorColor.copy(alpha = indicatorColor.alpha * a), indicatorShape) else Modifier
    Row(
        modifier.then(bg).clip(indicatorShape).indication(rippleSource, ripple()).padding(padding),
        horizontalArrangement = Arrangement.spacedBy(iconToLabel),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box { icon() }
        Box(if (labelAlpha < 1f) Modifier.alpha(labelAlpha.coerceIn(0f, 1f)) else Modifier) { label() }
    }
}

@Composable
internal fun navIndicatorProgress(selected: Boolean) =
    animateFloatAsState(targetValue = if (selected) 1f else 0f, animationSpec = NavMotion.defaultSpatial())

/**
 * The two ends of the paddings `WideNavigationRailItemDefaults.indicatorPadding` returns (AndroidX's
 * `DynamicPaddingValues`, an interface implementation OneSim's final `PaddingValues` class cannot have): the value
 * returned is the current end, registered here with both ends.
 */
internal object NavDynamicPadding {
    private val ends = LinkedHashMap<PaddingValues, Pair<PaddingValues, PaddingValues>>()

    fun register(current: PaddingValues, collapsed: PaddingValues, expanded: PaddingValues): PaddingValues {
        ends.remove(current)
        ends[current] = collapsed to expanded
        while (ends.size > 64) ends.remove(ends.keys.first())
        return current
    }

    fun ends(p: PaddingValues): Pair<PaddingValues, PaddingValues>? = ends[p]
}

/** A [ShortNavigationBar]'s placement for its items (the bar is a row the items size themselves in). */
internal class NavShortBarScope(
    val arrangement: ShortNavigationBarArrangement,
    val itemWeight: Float,
    /** The items currently in the bar (each registers a token). */
    val items: MutableList<Any>,
)

internal val LocalNavShortBarScope = compositionLocalOf<NavShortBarScope?> { null }

/** The min height a [WideNavigationRail] gives its items (animated), and whether it is expanded. */
internal class NavWideRailScope(val itemMinHeight: Dp, val itemSpacing: Dp)

internal val LocalNavWideRailScope = compositionLocalOf<NavWideRailScope?> { null }

/**
 * AndroidX `NavigationItem` (non-animated, used by [ShortNavigationBarItem]): a selectable item with the top-icon layout
 * (indicator `indicatorWidth` wide, label below) or the start-icon layout (icon and label on one pill).
 */
@Composable
internal fun NavigationItemImpl(
    selected: Boolean,
    onClick: () -> Unit,
    icon: @Composable () -> Unit,
    labelTextStyle: TextStyle,
    indicatorShape: Shape,
    indicatorWidth: Dp,
    indicatorHorizontalPadding: Dp,
    indicatorVerticalPadding: Dp,
    indicatorToLabelVerticalPadding: Dp,
    startIconToLabelHorizontalPadding: Dp,
    topIconItemVerticalPadding: Dp,
    textColor: Color,
    iconColor: Color,
    indicatorColor: Color,
    modifier: Modifier,
    enabled: Boolean,
    label: @Composable (() -> Unit)?,
    iconPosition: NavigationItemIconPosition,
    interactionSource: MutableInteractionSource,
    itemModifier: Modifier = Modifier,
    /** At least this fraction of the item's max width (a [ShortNavigationBar]'s centred arrangement). */
    minWidthFraction: Float = 0f,
) {
    val styledIcon: @Composable () -> Unit = { CompositionLocalProvider(LocalContentColor provides iconColor, content = icon) }
    val styledLabel: (@Composable () -> Unit)? = label?.let { l ->
        { NavProvideContentColorTextStyle(textColor, labelTextStyle, l) }
    }
    val width = remember { IntArray(1) }
    val isTop = iconPosition == NavigationItemIconPosition.Top
    val rippleSource: InteractionSource = if (isTop) {
        remember(interactionSource) {
            NavMappedInteractionSource(interactionSource) {
                Offset((width[0] - indicatorWidth.navRoundToPx()) / 2f, 12.dp.value * Guest.density)
            }
        }
    } else {
        interactionSource
    }
    val progress by navIndicatorProgress(selected)
    val p = progress.coerceAtLeast(0f)
    Box(
        modifier
            .selectable(selected = selected, interactionSource = interactionSource, indication = null, enabled = enabled, role = Role.Tab, onClick = onClick)
            .navObserveWidth(width)
            .then(itemModifier)
            .defaultMinSize(LocalMinimumInteractiveComponentSize.current, LocalMinimumInteractiveComponentSize.current),
        contentAlignment = if (isTop && styledLabel != null) Alignment.TopCenter else Alignment.Center,
    ) {
        if (minWidthFraction > 0f) Box(Modifier.fillMaxWidth(minWidthFraction))
        if (styledLabel == null || isTop) {
            val iconSize = 24.dp
            NavTopIconBody(
                modifier = if (styledLabel != null) Modifier.padding(vertical = topIconItemVerticalPadding) else Modifier,
                indicatorWidth = iconSize + indicatorHorizontalPadding * 2,
                indicatorHeight = iconSize + indicatorVerticalPadding * 2,
                indicatorShape = indicatorShape,
                indicatorColor = indicatorColor,
                widthProgress = p,
                indicatorAlpha = p,
                indicatorFromStart = false,
                rippleSource = rippleSource,
                icon = styledIcon,
                label = styledLabel,
                labelGap = indicatorToLabelVerticalPadding,
                labelAlpha = 1f,
            )
        } else {
            NavStartIconBody(
                modifier = Modifier,
                indicatorShape = indicatorShape,
                indicatorColor = indicatorColor,
                indicatorAlpha = p,
                padding = PaddingValues(horizontal = indicatorHorizontalPadding, vertical = indicatorVerticalPadding),
                iconToLabel = startIconToLabelHorizontalPadding,
                rippleSource = rippleSource,
                icon = styledIcon,
                label = styledLabel,
                labelAlpha = 1f,
            )
        }
    }
}

/**
 * AndroidX `AnimatedNavigationItem` (used by [WideNavigationRailItem]): the item switches between the top-icon layout
 * and the start-icon layout as [iconPosition] changes, the label fading out and back in (`4 (p - 0.5)^2`) around the
 * switch, as in AndroidX.
 */
@Composable
internal fun AnimatedNavigationItemImpl(
    selected: Boolean,
    onClick: () -> Unit,
    icon: @Composable () -> Unit,
    indicatorShape: Shape,
    topIconIndicatorWidth: Dp,
    topIconLabelTextStyle: TextStyle,
    startIconLabelTextStyle: TextStyle,
    indicatorPadding: PaddingValues,
    topIconIndicatorToLabelVerticalPadding: Dp,
    noLabelIndicatorPadding: Dp,
    startIconToLabelHorizontalPadding: Dp,
    itemHorizontalPadding: Dp,
    textColor: Color,
    iconColor: Color,
    indicatorColor: Color,
    modifier: Modifier,
    enabled: Boolean,
    label: @Composable (() -> Unit)?,
    iconPosition: NavigationItemIconPosition,
    interactionSource: MutableInteractionSource,
) {
    val styledIcon: @Composable () -> Unit = { CompositionLocalProvider(LocalContentColor provides iconColor, content = icon) }
    val width = remember { IntArray(1) }
    val isTop = iconPosition == NavigationItemIconPosition.Top
    val progress by navIndicatorProgress(selected)
    val p = progress.coerceAtLeast(0f)
    val positionProgress by animateFloatAsState(if (isTop) 0f else 1f, NavMotion.defaultSpatial())
    val pos = positionProgress.coerceIn(0f, 1f)
    val showTop = pos < 0.5f
    val animatedTextColor by animateColorAsState(textColor, NavMotion.defaultEffects())
    val rippleSource: InteractionSource = if (isTop) {
        remember(interactionSource) {
            NavMappedInteractionSource(interactionSource) {
                Offset((width[0] - topIconIndicatorWidth.navRoundToPx()) / 2f, 12.dp.value * Guest.density)
            }
        }
    } else {
        interactionSource
    }
    val rail = LocalNavWideRailScope.current
    val minHeight = rail?.itemMinHeight ?: LocalMinimumInteractiveComponentSize.current
    val labelAlpha = 4 * (pos - 0.5f) * (pos - 0.5f)
    // AndroidX interpolates a `DynamicPaddingValues` between its two ends; a plain PaddingValues is used for both.
    val ends = NavDynamicPadding.ends(indicatorPadding)
    val collapsedIndicatorPadding = ends?.first ?: indicatorPadding
    val expandedIndicatorPadding = ends?.second ?: indicatorPadding
    Box(
        modifier
            .selectable(selected = selected, interactionSource = interactionSource, indication = null, enabled = enabled, role = Role.Tab, onClick = onClick)
            .navObserveWidth(width)
            .defaultMinSize(LocalMinimumInteractiveComponentSize.current, minHeight),
        contentAlignment = if (showTop || label == null) Alignment.Center else Alignment.CenterStart,
    ) {
        val iconSize = 24.dp
        if (label == null) {
            // Icon only: a square-ish pill (`noLabelIndicatorPadding` around the icon) between the item paddings.
            NavTopIconBody(
                modifier = Modifier.padding(horizontal = itemHorizontalPadding),
                indicatorWidth = iconSize + noLabelIndicatorPadding * 2,
                indicatorHeight = iconSize + noLabelIndicatorPadding * 2,
                indicatorShape = indicatorShape,
                indicatorColor = indicatorColor,
                widthProgress = p,
                indicatorAlpha = p,
                indicatorFromStart = false,
                rippleSource = rippleSource,
                icon = styledIcon,
                label = null,
                labelGap = 0.dp,
                labelAlpha = 1f,
            )
        } else if (showTop) {
            val pad = collapsedIndicatorPadding
            val indicatorWidth = iconSize + pad.calculateStartPadding() + pad.calculateEndPadding()
            NavTopIconBody(
                modifier = Modifier.widthIn(min = indicatorWidth + itemHorizontalPadding * 2),
                indicatorWidth = indicatorWidth,
                indicatorHeight = iconSize + pad.calculateTopPadding() + pad.calculateBottomPadding(),
                indicatorShape = indicatorShape,
                indicatorColor = indicatorColor,
                widthProgress = p,
                indicatorAlpha = p,
                indicatorFromStart = true,
                rippleSource = rippleSource,
                icon = styledIcon,
                label = { NavProvideContentColorTextStyle(animatedTextColor, topIconLabelTextStyle, label) },
                labelGap = topIconIndicatorToLabelVerticalPadding,
                labelAlpha = labelAlpha,
            )
        } else {
            NavStartIconBody(
                modifier = Modifier.padding(start = itemHorizontalPadding),
                indicatorShape = indicatorShape,
                indicatorColor = indicatorColor,
                indicatorAlpha = p,
                padding = expandedIndicatorPadding,
                iconToLabel = startIconToLabelHorizontalPadding,
                rippleSource = rippleSource,
                icon = styledIcon,
                label = { NavProvideContentColorTextStyle(animatedTextColor, startIconLabelTextStyle, label) },
                labelAlpha = labelAlpha,
            )
        }
    }
}
