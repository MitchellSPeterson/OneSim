package androidx.compose.material3

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.spring
import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.background
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredHeight
import androidx.compose.foundation.layout.requiredWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.jsonNum
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import onesim.compose.Guest
import onesim.compose.NodeElement
import onesim.compose.NodeState
import onesim.compose.Primitive
import onesim.compose.WireCx

// ---- Tab positions and the indicator ---------------------------------------------------------------------------------

/**
 * A tab's place in its row: [left] edge, [width] and its content's width. OneSim measures tabs on the host, so the
 * positions a deprecated `TabRow`/`ScrollableTabRow` indicator lambda receives are estimates (equal widths of the
 * screen, or [TabRowDefaults.ScrollableTabRowMinTabWidth] per tab when scrollable); `tabIndicatorOffset` places the
 * indicator at the measured position.
 */
@Immutable
class TabPosition internal constructor(val left: Dp, val width: Dp, val contentWidth: Dp) {
    internal var index: Int = -1

    val right: Dp get() = left + width

    override fun equals(other: Any?) = other is TabPosition && left == other.left && width == other.width && contentWidth == other.contentWidth
    override fun hashCode() = (left.hashCode() * 31 + width.hashCode()) * 31 + contentWidth.hashCode()
    override fun toString() = "TabPosition(left=$left, right=$right, width=$width, contentWidth=$contentWidth)"
}

/** Scope of a [PrimaryTabRow] / [SecondaryTabRow] indicator. */
interface TabIndicatorScope {
    /**
     * Places the indicator under the tab at [selectedTabIndex] with that tab's width ([matchContentSize]: its content's
     * width, centred), sliding there with the standard spatial spring when the selection changes.
     */
    fun Modifier.tabIndicatorOffset(selectedTabIndex: Int, matchContentSize: Boolean = false): Modifier
}

private object TabIndicatorScopeImpl : TabIndicatorScope {
    override fun Modifier.tabIndicatorOffset(selectedTabIndex: Int, matchContentSize: Boolean): Modifier =
        this then TabIndicatorElement(selectedTabIndex, matchContentSize, spring(dampingRatio = 0.9f, stiffness = 700f, visibilityThreshold = 0.001f))
}

/** `MotionSchemeKeyTokens.DefaultSpatial` of the standard motion scheme. */
private fun <T> defaultSpatial(): FiniteAnimationSpec<T> = spring(dampingRatio = 0.9f, stiffness = 700f)

private class TabIndicatorElement(val index: Int, val matchContent: Boolean, val spec: FiniteAnimationSpec<Float>) : NodeElement {
    override fun create(): NodeState = TabIndicatorState()

    override fun wire(state: NodeState, cx: WireCx) {
        val s = state as TabIndicatorState
        s.update(index, spec)
        val from = if (s.progress.floatValue < 1f) ""","from":[${s.fromA},${s.fromB},${jsonNum(s.fromP)}],"progress":${jsonNum(s.progress.floatValue)}""" else ""
        cx.add("""{"tabIndicator":{"index":$index,"match":"${if (matchContent) "content" else "tab"}"$from}}""")
    }

    override fun equals(other: Any?) = other is TabIndicatorElement && index == other.index && matchContent == other.matchContent && spec == other.spec
    override fun hashCode() = (index * 31 + matchContent.hashCode()) * 31 + spec.hashCode()
}

/** The indicator's slide: from a place between tabs [fromA] and [fromB] ([fromP]) to the selected tab. */
private class TabIndicatorState : NodeState() {
    var target = -1
    var fromA = 0
    var fromB = 0
    var fromP = 0f
    val progress = mutableFloatStateOf(1f)
    private var job: kotlinx.coroutines.Job? = null

    fun update(index: Int, spec: FiniteAnimationSpec<Float>) {
        if (index == target) return
        if (target < 0) {
            target = index
            return
        }
        // Start from where the indicator is now (approximated between the previous slide's ends).
        if (progress.floatValue < 1f) {
            fromA = fromB
            fromB = target
            fromP = progress.floatValue
        } else {
            fromA = target
            fromB = target
            fromP = 0f
        }
        target = index
        progress.floatValue = 0f
        job?.cancel()
        job = Guest.scope.launch {
            val a = Animatable(0f, 0.001f)
            a.animateTo(1f, spec) { progress.floatValue = value }
            progress.floatValue = 1f
        }
    }

    override fun detach() {
        job?.cancel()
    }
}

// ---- Tab rows --------------------------------------------------------------------------------------------------------

/** The selected tab's index changes counted, so the host centres a scrollable row on each new selection. */
private class ScrollSelection {
    var selected = -1
    var previous = -1
    var seq = 0
    val progress = mutableFloatStateOf(1f)
    var job: kotlinx.coroutines.Job? = null
}

@Composable
private fun FixedTabRow(
    modifier: Modifier,
    containerColor: Color,
    contentColor: Color,
    indicator: @Composable () -> Unit,
    divider: @Composable () -> Unit,
    tabs: @Composable () -> Unit,
) {
    Surface(modifier = modifier.selectableGroup(), color = containerColor, contentColor = contentColor) {
        Primitive("TabRow", Modifier.fillMaxWidth(), "") {
            tabs()
            Box(Modifier) { divider() }
            Box(Modifier) { indicator() }
        }
    }
}

@Composable
private fun ScrollingTabRow(
    selectedTabIndex: Int,
    modifier: Modifier,
    containerColor: Color,
    contentColor: Color,
    edgePadding: Dp,
    minTabWidth: Dp,
    indicator: @Composable () -> Unit,
    divider: @Composable () -> Unit,
    tabs: @Composable () -> Unit,
) {
    val sel = remember { ScrollSelection() }
    if (sel.selected != selectedTabIndex) {
        if (sel.selected >= 0) {
            sel.previous = sel.selected
            sel.seq++
            sel.progress.floatValue = 0f
            sel.job?.cancel()
            sel.job = Guest.scope.launch {
                Animatable(0f, 0.001f).animateTo(1f, defaultSpatial()) { sel.progress.floatValue = value }
                sel.progress.floatValue = 1f
            }
        }
        sel.selected = selectedTabIndex
    }
    val q = sel.progress.floatValue
    val scroll = """"scroll":{"seq":${sel.seq}${if (q < 1f && sel.previous >= 0) ""","from":${sel.previous},"progress":${jsonNum(q)}""" else ""}}"""
    Surface(modifier = modifier, color = containerColor, contentColor = contentColor) {
        Primitive(
            "TabRow",
            Modifier.fillMaxWidth().selectableGroup(),
            """"scrollable":true,"selected":$selectedTabIndex,"edgePadding":${jsonNum(edgePadding.value)},"minTabWidth":${jsonNum(minTabWidth.value)},$scroll""",
        ) {
            tabs()
            Box(Modifier) { divider() }
            Box(Modifier) { indicator() }
        }
    }
}

/** Fixed tabs of equal width with Material's primary indicator (the content's width, 3 dp, rounded). */
@Composable
fun PrimaryTabRow(
    selectedTabIndex: Int,
    modifier: Modifier = Modifier,
    containerColor: Color = TabRowDefaults.primaryContainerColor,
    contentColor: Color = TabRowDefaults.primaryContentColor,
    indicator: @Composable TabIndicatorScope.() -> Unit = {
        TabRowDefaults.PrimaryIndicator(modifier = Modifier.tabIndicatorOffset(selectedTabIndex, matchContentSize = true), width = Dp.Unspecified)
    },
    divider: @Composable () -> Unit = @Composable { HorizontalDivider() },
    tabs: @Composable () -> Unit,
) = FixedTabRow(modifier, containerColor, contentColor, { TabIndicatorScopeImpl.indicator() }, divider, tabs)

/** Fixed tabs of equal width with the secondary indicator (the tab's width, 2 dp). */
@Composable
fun SecondaryTabRow(
    selectedTabIndex: Int,
    modifier: Modifier = Modifier,
    containerColor: Color = TabRowDefaults.secondaryContainerColor,
    contentColor: Color = TabRowDefaults.secondaryContentColor,
    indicator: @Composable TabIndicatorScope.() -> Unit = @Composable {
        TabRowDefaults.SecondaryIndicator(Modifier.tabIndicatorOffset(selectedTabIndex, matchContentSize = false))
    },
    divider: @Composable () -> Unit = @Composable { HorizontalDivider() },
    tabs: @Composable () -> Unit,
) = FixedTabRow(modifier, containerColor, contentColor, { TabIndicatorScopeImpl.indicator() }, divider, tabs)

@Composable
fun PrimaryScrollableTabRow(
    selectedTabIndex: Int,
    modifier: Modifier = Modifier,
    scrollState: ScrollState = rememberScrollState(),
    containerColor: Color = TabRowDefaults.primaryContainerColor,
    contentColor: Color = TabRowDefaults.primaryContentColor,
    edgePadding: Dp = TabRowDefaults.ScrollableTabRowEdgeStartPadding,
    indicator: @Composable TabIndicatorScope.() -> Unit = @Composable {
        TabRowDefaults.PrimaryIndicator(Modifier.tabIndicatorOffset(selectedTabIndex, matchContentSize = true), width = Dp.Unspecified)
    },
    divider: @Composable () -> Unit = @Composable { HorizontalDivider() },
    minTabWidth: Dp = TabRowDefaults.ScrollableTabRowMinTabWidth,
    tabs: @Composable () -> Unit,
) = ScrollingTabRow(selectedTabIndex, modifier, containerColor, contentColor, edgePadding, minTabWidth, { TabIndicatorScopeImpl.indicator() }, divider, tabs)

@Composable
fun SecondaryScrollableTabRow(
    selectedTabIndex: Int,
    modifier: Modifier = Modifier,
    scrollState: ScrollState = rememberScrollState(),
    containerColor: Color = TabRowDefaults.secondaryContainerColor,
    contentColor: Color = TabRowDefaults.secondaryContentColor,
    edgePadding: Dp = TabRowDefaults.ScrollableTabRowEdgeStartPadding,
    indicator: @Composable TabIndicatorScope.() -> Unit = @Composable {
        TabRowDefaults.SecondaryIndicator(Modifier.tabIndicatorOffset(selectedTabIndex, matchContentSize = false))
    },
    divider: @Composable () -> Unit = @Composable { HorizontalDivider() },
    minTabWidth: Dp = TabRowDefaults.ScrollableTabRowMinTabWidth,
    tabs: @Composable () -> Unit,
) = ScrollingTabRow(selectedTabIndex, modifier, containerColor, contentColor, edgePadding, minTabWidth, { TabIndicatorScopeImpl.indicator() }, divider, tabs)

/** Estimated positions for the deprecated rows' indicator lambdas (the host places the indicator exactly). */
private fun estimatedPositions(count: Int, scrollable: Boolean, edgePadding: Dp): List<TabPosition> {
    if (count <= 0) return emptyList()
    return List(count) { i ->
        val p = if (scrollable) {
            TabPosition(edgePadding + TabRowDefaults.ScrollableTabRowMinTabWidth * i, TabRowDefaults.ScrollableTabRowMinTabWidth, 24.dp)
        } else {
            val w = Dp(Guest.screenWidth / count)
            TabPosition(w * i, w, 24.dp)
        }
        p.also { it.index = i }
    }
}

@Deprecated("Replaced with PrimaryTabRow and SecondaryTabRow.", ReplaceWith("SecondaryTabRow(selectedTabIndex, modifier, containerColor, contentColor, indicator, divider, tabs)"))
@Composable
fun TabRow(
    selectedTabIndex: Int,
    modifier: Modifier = Modifier,
    containerColor: Color = TabRowDefaults.primaryContainerColor,
    contentColor: Color = TabRowDefaults.primaryContentColor,
    indicator: @Composable (tabPositions: List<TabPosition>) -> Unit = @Composable { tabPositions ->
        if (selectedTabIndex < tabPositions.size) {
            @Suppress("DEPRECATION")
            TabRowDefaults.SecondaryIndicator(with(TabRowDefaults) { Modifier.tabIndicatorOffset(tabPositions[selectedTabIndex]) })
        }
    },
    divider: @Composable () -> Unit = @Composable { HorizontalDivider() },
    tabs: @Composable () -> Unit,
) {
    val count = remember { TabCounter() }
    FixedTabRow(modifier, containerColor, contentColor, { indicator(estimatedPositions(maxOf(count.count, selectedTabIndex + 1), false, 0.dp)) }, divider) {
        count.counting(tabs)
    }
}

@Deprecated("Replaced with PrimaryScrollableTabRow and SecondaryScrollableTabRow tab variants.", ReplaceWith("SecondaryScrollableTabRow(selectedTabIndex, modifier, containerColor, contentColor, edgePadding, indicator, divider, tabs)"))
@Composable
fun ScrollableTabRow(
    selectedTabIndex: Int,
    modifier: Modifier = Modifier,
    containerColor: Color = TabRowDefaults.primaryContainerColor,
    contentColor: Color = TabRowDefaults.primaryContentColor,
    edgePadding: Dp = TabRowDefaults.ScrollableTabRowEdgeStartPadding,
    indicator: @Composable (tabPositions: List<TabPosition>) -> Unit = @Composable { tabPositions ->
        @Suppress("DEPRECATION")
        TabRowDefaults.SecondaryIndicator(with(TabRowDefaults) { Modifier.tabIndicatorOffset(tabPositions[selectedTabIndex]) })
    },
    divider: @Composable () -> Unit = @Composable { HorizontalDivider() },
    tabs: @Composable () -> Unit,
) {
    val count = remember { TabCounter() }
    ScrollingTabRow(
        selectedTabIndex, modifier, containerColor, contentColor, edgePadding, TabRowDefaults.ScrollableTabRowMinTabWidth,
        { indicator(estimatedPositions(maxOf(count.count, selectedTabIndex + 1), true, edgePadding)) }, divider,
    ) { count.counting(tabs) }
}

/** Counts the tabs of the previous composition of a `tabs` slot (indicator positions need the count). */
private class TabCounter {
    var count = 0

    @Composable
    fun counting(tabs: @Composable () -> Unit) {
        val before = TabCount.current
        TabCount.current = 0
        tabs()
        // Tabs skipped in a recomposition do not count themselves: keep the last full count.
        if (TabCount.current > 0) count = TabCount.current
        TabCount.current = before
    }
}

/** Tabs composed in the current `tabs` slot ([Tab] and [LeadingIconTab] count themselves). */
internal object TabCount {
    var current = 0
}

// ---- Defaults --------------------------------------------------------------------------------------------------------

object TabRowDefaults {
    val ScrollableTabRowMinTabWidth: Dp = 90.dp
    val ScrollableTabRowEdgeStartPadding: Dp = 52.dp

    @Deprecated("Use TabRowDefaults.primaryContainerColor instead", ReplaceWith("primaryContainerColor"))
    val containerColor: Color @Composable get() = MaterialTheme.colorScheme.surface
    val primaryContainerColor: Color @Composable get() = MaterialTheme.colorScheme.surface
    val secondaryContainerColor: Color @Composable get() = MaterialTheme.colorScheme.surface

    @Deprecated("Use TabRowDefaults.primaryContentColor instead", ReplaceWith("primaryContentColor"))
    val contentColor: Color @Composable get() = MaterialTheme.colorScheme.primary
    val primaryContentColor: Color @Composable get() = MaterialTheme.colorScheme.primary
    val secondaryContentColor: Color @Composable get() = MaterialTheme.colorScheme.onSurface

    @Deprecated("Use SecondaryIndicator instead.", ReplaceWith("SecondaryIndicator(modifier, height, color)"))
    @Composable
    fun Indicator(modifier: Modifier = Modifier, height: Dp = 3.dp, color: Color = MaterialTheme.colorScheme.primary) {
        Box(modifier.fillMaxWidth().height(height).background(color = color))
    }

    /** 3 dp tall, rounded, [width] wide (`Dp.Unspecified`: the width `tabIndicatorOffset` gives). */
    @Composable
    fun PrimaryIndicator(
        modifier: Modifier = Modifier,
        width: Dp = 24.dp,
        height: Dp = 3.dp,
        color: Color = MaterialTheme.colorScheme.primary,
        shape: Shape = RoundedCornerShape(3.dp),
    ) {
        Spacer(modifier.requiredHeight(height).requiredWidth(width).background(color = color, shape = shape))
    }

    @Composable
    fun SecondaryIndicator(modifier: Modifier = Modifier, height: Dp = 3.dp, color: Color = MaterialTheme.colorScheme.primary) {
        Box(modifier.fillMaxWidth().height(height).background(color = color))
    }

    /** The deprecated rows' indicator placement: under [currentTabPosition] with its full width, sliding between tabs. */
    @Deprecated(
        "Solely for use alongside deprecated TabRowDefaults.Indicator method. For recommended PrimaryIndicator and SecondaryIndicator methods, please use TabIndicatorScope.tabIndicatorOffset method.",
        level = DeprecationLevel.WARNING,
    )
    fun Modifier.tabIndicatorOffset(currentTabPosition: TabPosition): Modifier =
        if (currentTabPosition.index < 0) this else this then TabIndicatorElement(currentTabPosition.index, false, defaultSpatial())
}

// ---- Tabs ------------------------------------------------------------------------------------------------------------

private val HorizontalTextPadding = 16.dp
private val SmallTabHeight = 48.dp
private val LargeTabHeight = 72.dp
private val TextDistanceFromLeadingIcon = 8.dp
private val IconDistanceFromBaseline = 20.dp
private val SingleLineTextBaselineWithIcon = 14.dp

@Composable
private fun TabTransition(activeColor: Color, inactiveColor: Color, selected: Boolean, content: @Composable () -> Unit) {
    // Fade in with the default effects spring, out with the fast one (standard motion scheme).
    val color by animateColorAsState(
        if (selected) activeColor else inactiveColor,
        if (selected) spring(dampingRatio = 1f, stiffness = 1600f) else spring(dampingRatio = 1f, stiffness = 3800f),
    )
    CompositionLocalProvider(LocalContentColor provides color, content = content)
}

/** A tab with a [text] label and/or an [icon]: 48 dp with one of them, 72 dp with both (text baseline 14 dp + indicator above the bottom). */
@Composable
fun Tab(
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    text: @Composable (() -> Unit)? = null,
    icon: @Composable (() -> Unit)? = null,
    selectedContentColor: Color = LocalContentColor.current,
    unselectedContentColor: Color = selectedContentColor,
    interactionSource: MutableInteractionSource? = null,
) {
    val style = MaterialTheme.typography.titleSmall.copy(textAlign = TextAlign.Center)
    Tab(selected, onClick, modifier, enabled, selectedContentColor, unselectedContentColor, interactionSource) {
        TabBaselineLayout(text?.let { t -> @Composable { ProvideTextStyle(style, t) } }, icon, style)
    }
}

@Composable
fun LeadingIconTab(
    selected: Boolean,
    onClick: () -> Unit,
    text: @Composable () -> Unit,
    icon: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    selectedContentColor: Color = LocalContentColor.current,
    unselectedContentColor: Color = selectedContentColor,
    interactionSource: MutableInteractionSource? = null,
) {
    TabCount.current++
    val ripple = ripple(bounded = true, color = selectedContentColor)
    TabTransition(selectedContentColor, unselectedContentColor, selected) {
        Row(
            modifier.height(SmallTabHeight)
                .selectable(selected, interactionSource, ripple, enabled, Role.Tab, onClick)
                .padding(horizontal = HorizontalTextPadding)
                .fillMaxWidth(),
            horizontalArrangement = Arrangement.Center,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            icon()
            Spacer(Modifier.requiredWidth(TextDistanceFromLeadingIcon))
            ProvideTextStyle(MaterialTheme.typography.titleSmall.copy(textAlign = TextAlign.Center), text)
        }
    }
}

@Composable
fun Tab(
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    selectedContentColor: Color = LocalContentColor.current,
    unselectedContentColor: Color = selectedContentColor,
    interactionSource: MutableInteractionSource? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    TabCount.current++
    val ripple = ripple(bounded = true, color = selectedContentColor)
    TabTransition(selectedContentColor, unselectedContentColor, selected) {
        Column(
            modifier.selectable(selected, interactionSource, ripple, enabled, Role.Tab, onClick).fillMaxWidth(),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
            content = content,
        )
    }
}

/**
 * Material's `TabBaselineLayout` without baselines: a lone text or icon is centred in 48 dp; with both (72 dp) the icon
 * sits 11 dp from the top (`72 - 3 - 14 - 20 - 24`) and the text's first baseline 17 dp above the bottom, its top
 * derived from the style's line height and Roboto's ascent and descent.
 */
@Composable
private fun TabBaselineLayout(text: @Composable (() -> Unit)?, icon: @Composable (() -> Unit)?, style: androidx.compose.ui.text.TextStyle) {
    when {
        text != null && icon != null -> {
            val lineHeight = style.lineHeight.value.takeIf { !it.isNaN() } ?: (style.fontSize.value * 1.2f)
            val fontSize = style.fontSize.value
            // Baseline within the text's line box (half-leading above Roboto's ascent).
            val baseline = (lineHeight - (0.927f + 0.244f) * fontSize) / 2f + 0.927f * fontSize
            val textTop = (LargeTabHeight.value - 3f - SingleLineTextBaselineWithIcon.value) - baseline
            val iconTop = (LargeTabHeight.value - 3f - SingleLineTextBaselineWithIcon.value) - IconDistanceFromBaseline.value - 24f
            Box(Modifier.fillMaxWidth().heightIn(min = LargeTabHeight)) {
                Box(Modifier.align(Alignment.TopCenter).offset(y = Dp(iconTop))) { icon() }
                Box(Modifier.align(Alignment.TopCenter).offset(y = Dp(textTop)).padding(horizontal = HorizontalTextPadding)) { text() }
            }
        }
        text != null -> Box(Modifier.heightIn(min = SmallTabHeight).padding(horizontal = HorizontalTextPadding), contentAlignment = Alignment.Center) { text() }
        icon != null -> Box(Modifier.heightIn(min = SmallTabHeight), contentAlignment = Alignment.Center) { icon() }
        else -> Box(Modifier.heightIn(min = SmallTabHeight))
    }
}
