package androidx.compose.material3

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.DecayAnimationSpec
import androidx.compose.animation.core.FastOutLinearInEasing
import androidx.compose.animation.core.SpringSpec
import androidx.compose.animation.rememberSplineBasedDecay
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.ScrollableState
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredHeight
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.takeOrElse
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.dp
import onesim.compose.Guest
import kotlin.math.abs
import kotlin.math.absoluteValue
import kotlin.math.cbrt
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.pow
import kotlin.math.roundToInt

// Material 3 app bars (`androidx.compose.material3` 1.5.0-beta01 `AppBar.kt`), built from Box/Row/Column.
//
// AndroidX lays the bars out with custom `Layout`s that measure their slots against the incoming constraints and then
// place them in a box that shrinks with the scroll offset (clipped). The guest cannot measure, so each layer is laid
// out at its full height in an overflow frame ([OverflowFrame]) and positioned in whole pixels from the scroll state,
// which reproduces the same geometry; titles anchored to the bottom by their baseline use the Roboto line metrics of
// the title's text style ([lineBox]).

// ---- Tokens (material3 1.5.0-beta01 `tokens/AppBar*Tokens`, `BottomAppBarTokens`, `DockedToolbarTokens`) --------------

private val AppBarSmallContainerHeight = 64.dp
private val AppBarMediumContainerHeight = 112.dp
private val AppBarMediumFlexibleContainerHeight = 112.dp
private val AppBarMediumFlexibleLargeContainerHeight = 136.dp
private val AppBarLargeContainerHeight = 152.dp
private val AppBarLargeFlexibleContainerHeight = 120.dp
private val AppBarLargeFlexibleLargeContainerHeight = 152.dp
private val BottomAppBarContainerHeight = 80.dp
private val DockedToolbarContainerHeight = 64.dp
private val DockedToolbarLeadingSpace = 16.dp
private val DockedToolbarTrailingSpace = 16.dp
private val DockedToolbarMaxSpacing = 32.dp

// Text styles of the bars (`AppBar*Tokens.TitleFont` / `SubtitleFont`).
private val Typography.smallTitle get() = titleLarge
private val Typography.smallSubtitle get() = labelMedium
private val Typography.mediumTitle get() = headlineSmall
private val Typography.mediumFlexibleTitle get() = headlineMedium
private val Typography.mediumFlexibleSubtitle get() = labelLarge
private val Typography.largeTitle get() = headlineMedium
private val Typography.largeFlexibleTitle get() = displaySmall
private val Typography.largeFlexibleSubtitle get() = titleMedium

// Standard motion scheme (`StandardMotionTokens`): DefaultEffects snaps top app bars and fades their container color,
// FastSpatial snaps bottom app bars.
private val DefaultEffectsFloat: AnimationSpec<Float> = SpringSpec(dampingRatio = 1f, stiffness = 1600f)
private val DefaultEffectsColor: AnimationSpec<Color> = SpringSpec(dampingRatio = 1f, stiffness = 1600f)
private val FastSpatialFloat: AnimationSpec<Float> = SpringSpec(dampingRatio = 0.9f, stiffness = 1400f)

/** An easing used to fade the title in the top (collapsed) row of a two-row app bar. */
internal val TopTitleAlphaEasing = CubicBezierEasing(.8f, 0f, .8f, .15f)

private val MediumTitleBottomPadding = 24.dp
private val LargeTitleBottomPadding = 28.dp
private val TopAppBarHorizontalPadding = 4.dp

/** A title inset when there is no navigation icon (also the minimum space before the title). */
private val TopAppBarTitleInset = 16.dp - TopAppBarHorizontalPadding

// Padding minus IconButton's min touch target expansion.
private val BottomAppBarHorizontalPadding = 16.dp - 12.dp
internal val BottomAppBarVerticalPadding = 16.dp - 12.dp

// Padding minus content padding.
private val FABHorizontalPadding = 16.dp - BottomAppBarHorizontalPadding
private val FABVerticalPadding = 12.dp - BottomAppBarVerticalPadding

private val Dp.isFiniteSpecified: Boolean get() = isSpecified && value.isFinite()

// ---- Top app bars ------------------------------------------------------------------------------------------------------

@Deprecated(message = "Deprecated in favor of TopAppBar with contentPadding parameter", level = DeprecationLevel.HIDDEN)
@Composable
fun TopAppBar(
    title: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    navigationIcon: @Composable () -> Unit = {},
    actions: @Composable RowScope.() -> Unit = {},
    expandedHeight: Dp = TopAppBarDefaults.TopAppBarExpandedHeight,
    windowInsets: WindowInsets = TopAppBarDefaults.windowInsets,
    colors: TopAppBarColors = TopAppBarDefaults.topAppBarColors(),
    scrollBehavior: TopAppBarScrollBehavior? = null,
): Unit = TopAppBar(
    title = title, modifier = modifier, navigationIcon = navigationIcon, actions = actions, expandedHeight = expandedHeight,
    windowInsets = windowInsets, colors = colors, scrollBehavior = scrollBehavior, contentPadding = TopAppBarDefaults.ContentPadding,
)

/** Small top app bar: [expandedHeight] (64 dp) under the status bar, title in titleLarge. */
@Composable
fun TopAppBar(
    title: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    navigationIcon: @Composable () -> Unit = {},
    actions: @Composable RowScope.() -> Unit = {},
    expandedHeight: Dp = TopAppBarDefaults.TopAppBarExpandedHeight,
    windowInsets: WindowInsets = TopAppBarDefaults.windowInsets,
    colors: TopAppBarColors = TopAppBarDefaults.topAppBarColors(),
    scrollBehavior: TopAppBarScrollBehavior? = null,
    contentPadding: PaddingValues = TopAppBarDefaults.ContentPadding,
): Unit = SingleRowTopAppBar(
    modifier = modifier,
    title = title,
    titleTextStyle = MaterialTheme.typography.smallTitle,
    subtitle = null,
    subtitleTextStyle = TextStyle.Default,
    titleHorizontalAlignment = Alignment.Start,
    navigationIcon = navigationIcon,
    actions = actions,
    expandedHeight = if (expandedHeight.isFiniteSpecified) expandedHeight else TopAppBarDefaults.TopAppBarExpandedHeight,
    contentPadding = contentPadding,
    windowInsets = windowInsets,
    colors = colors,
    scrollBehavior = scrollBehavior,
)

@Deprecated(message = "Deprecated in favor of CenterAlignedTopAppBar with contentPadding parameter", level = DeprecationLevel.HIDDEN)
@Composable
fun CenterAlignedTopAppBar(
    title: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    navigationIcon: @Composable () -> Unit = {},
    actions: @Composable RowScope.() -> Unit = {},
    expandedHeight: Dp = TopAppBarDefaults.TopAppBarExpandedHeight,
    windowInsets: WindowInsets = TopAppBarDefaults.windowInsets,
    colors: TopAppBarColors = TopAppBarDefaults.topAppBarColors(),
    scrollBehavior: TopAppBarScrollBehavior? = null,
): Unit = CenterAlignedTopAppBar(
    title = title, modifier = modifier, navigationIcon = navigationIcon, actions = actions, expandedHeight = expandedHeight,
    contentPadding = TopAppBarDefaults.ContentPadding, windowInsets = windowInsets, colors = colors, scrollBehavior = scrollBehavior,
)

/** Small top app bar with the title centred on the bar's width. */
@Composable
fun CenterAlignedTopAppBar(
    title: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    navigationIcon: @Composable () -> Unit = {},
    actions: @Composable RowScope.() -> Unit = {},
    expandedHeight: Dp = TopAppBarDefaults.TopAppBarExpandedHeight,
    windowInsets: WindowInsets = TopAppBarDefaults.windowInsets,
    colors: TopAppBarColors = TopAppBarDefaults.topAppBarColors(),
    scrollBehavior: TopAppBarScrollBehavior? = null,
    contentPadding: PaddingValues = TopAppBarDefaults.ContentPadding,
): Unit = SingleRowTopAppBar(
    modifier = modifier,
    title = title,
    titleTextStyle = MaterialTheme.typography.smallTitle,
    subtitle = null,
    subtitleTextStyle = TextStyle.Default,
    titleHorizontalAlignment = Alignment.CenterHorizontally,
    navigationIcon = navigationIcon,
    actions = actions,
    expandedHeight = if (expandedHeight.isFiniteSpecified) expandedHeight else TopAppBarDefaults.TopAppBarExpandedHeight,
    contentPadding = contentPadding,
    windowInsets = windowInsets,
    colors = colors,
    scrollBehavior = scrollBehavior,
)

/** Small top app bar with a [subtitle] (labelMedium, onSurfaceVariant) under the title. */
@Composable
fun TopAppBar(
    title: @Composable () -> Unit,
    subtitle: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    navigationIcon: @Composable () -> Unit = {},
    actions: @Composable RowScope.() -> Unit = {},
    titleHorizontalAlignment: Alignment.Horizontal = Alignment.Start,
    expandedHeight: Dp = TopAppBarDefaults.TopAppBarExpandedHeight,
    windowInsets: WindowInsets = TopAppBarDefaults.windowInsets,
    colors: TopAppBarColors = TopAppBarDefaults.topAppBarColors(),
    scrollBehavior: TopAppBarScrollBehavior? = null,
    contentPadding: PaddingValues = TopAppBarDefaults.ContentPadding,
): Unit = SingleRowTopAppBar(
    modifier = modifier,
    title = title,
    titleTextStyle = MaterialTheme.typography.smallTitle,
    subtitle = subtitle,
    subtitleTextStyle = MaterialTheme.typography.smallSubtitle,
    titleHorizontalAlignment = titleHorizontalAlignment,
    navigationIcon = navigationIcon,
    actions = actions,
    expandedHeight = if (expandedHeight.isFiniteSpecified) expandedHeight else TopAppBarDefaults.TopAppBarExpandedHeight,
    contentPadding = contentPadding,
    windowInsets = windowInsets,
    colors = colors,
    scrollBehavior = scrollBehavior,
)

/** Medium top app bar: a 64 dp row with the icons over a row with the headlineSmall title, collapsing to 64 dp. */
@Composable
fun MediumTopAppBar(
    title: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    navigationIcon: @Composable () -> Unit = {},
    actions: @Composable RowScope.() -> Unit = {},
    collapsedHeight: Dp = TopAppBarDefaults.MediumAppBarCollapsedHeight,
    expandedHeight: Dp = TopAppBarDefaults.MediumAppBarExpandedHeight,
    windowInsets: WindowInsets = TopAppBarDefaults.windowInsets,
    colors: TopAppBarColors = TopAppBarDefaults.topAppBarColors(),
    scrollBehavior: TopAppBarScrollBehavior? = null,
): Unit = TwoRowsTopAppBar(
    modifier = modifier,
    title = title,
    titleTextStyle = MaterialTheme.typography.mediumTitle,
    smallTitleTextStyle = MaterialTheme.typography.smallTitle,
    titleBottomPadding = MediumTitleBottomPadding,
    smallTitle = title,
    subtitle = null,
    subtitleTextStyle = TextStyle.Default,
    smallSubtitle = null,
    smallSubtitleTextStyle = TextStyle.Default,
    titleHorizontalAlignment = Alignment.Start,
    navigationIcon = navigationIcon,
    actions = actions,
    collapsedHeight = if (collapsedHeight.isFiniteSpecified) collapsedHeight else TopAppBarDefaults.MediumAppBarCollapsedHeight,
    expandedHeight = if (expandedHeight.isFiniteSpecified) expandedHeight else TopAppBarDefaults.MediumAppBarExpandedHeight,
    windowInsets = windowInsets,
    colors = colors,
    scrollBehavior = scrollBehavior,
)

/** Medium flexible top app bar (expressive): headlineMedium title, optional labelLarge [subtitle]. */
@Composable
fun MediumFlexibleTopAppBar(
    title: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    subtitle: (@Composable () -> Unit)? = null,
    navigationIcon: @Composable () -> Unit = {},
    actions: @Composable RowScope.() -> Unit = {},
    titleHorizontalAlignment: Alignment.Horizontal = Alignment.Start,
    collapsedHeight: Dp = TopAppBarDefaults.MediumAppBarCollapsedHeight,
    expandedHeight: Dp = if (subtitle != null) {
        TopAppBarDefaults.MediumFlexibleAppBarWithSubtitleExpandedHeight
    } else {
        TopAppBarDefaults.MediumFlexibleAppBarWithoutSubtitleExpandedHeight
    },
    windowInsets: WindowInsets = TopAppBarDefaults.windowInsets,
    colors: TopAppBarColors = TopAppBarDefaults.topAppBarColors(),
    scrollBehavior: TopAppBarScrollBehavior? = null,
): Unit = TwoRowsTopAppBar(
    modifier = modifier,
    title = title,
    titleTextStyle = MaterialTheme.typography.mediumFlexibleTitle,
    smallTitleTextStyle = MaterialTheme.typography.smallTitle,
    titleBottomPadding = MediumTitleBottomPadding,
    smallTitle = title,
    subtitle = subtitle ?: {},
    subtitleTextStyle = MaterialTheme.typography.mediumFlexibleSubtitle,
    smallSubtitle = subtitle ?: {},
    smallSubtitleTextStyle = MaterialTheme.typography.smallSubtitle,
    titleHorizontalAlignment = titleHorizontalAlignment,
    navigationIcon = navigationIcon,
    actions = actions,
    collapsedHeight = if (collapsedHeight.isFiniteSpecified) collapsedHeight else TopAppBarDefaults.MediumAppBarCollapsedHeight,
    expandedHeight = if (expandedHeight.isFiniteSpecified) {
        expandedHeight
    } else if (subtitle != null) {
        TopAppBarDefaults.MediumFlexibleAppBarWithSubtitleExpandedHeight
    } else {
        TopAppBarDefaults.MediumFlexibleAppBarWithoutSubtitleExpandedHeight
    },
    windowInsets = windowInsets,
    colors = colors,
    scrollBehavior = scrollBehavior,
    hasSubtitle = subtitle != null,
)

/** Large top app bar: a 64 dp row with the icons over a row with the headlineMedium title, collapsing to 64 dp. */
@Composable
fun LargeTopAppBar(
    title: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    navigationIcon: @Composable () -> Unit = {},
    actions: @Composable RowScope.() -> Unit = {},
    collapsedHeight: Dp = TopAppBarDefaults.LargeAppBarCollapsedHeight,
    expandedHeight: Dp = TopAppBarDefaults.LargeAppBarExpandedHeight,
    windowInsets: WindowInsets = TopAppBarDefaults.windowInsets,
    colors: TopAppBarColors = TopAppBarDefaults.topAppBarColors(),
    scrollBehavior: TopAppBarScrollBehavior? = null,
): Unit = TwoRowsTopAppBar(
    title = title,
    titleTextStyle = MaterialTheme.typography.largeTitle,
    smallTitleTextStyle = MaterialTheme.typography.smallTitle,
    titleBottomPadding = LargeTitleBottomPadding,
    smallTitle = title,
    modifier = modifier,
    subtitle = null,
    subtitleTextStyle = TextStyle.Default,
    smallSubtitle = null,
    smallSubtitleTextStyle = TextStyle.Default,
    titleHorizontalAlignment = Alignment.Start,
    navigationIcon = navigationIcon,
    actions = actions,
    collapsedHeight = if (collapsedHeight.isFiniteSpecified) collapsedHeight else TopAppBarDefaults.LargeAppBarCollapsedHeight,
    expandedHeight = if (expandedHeight.isFiniteSpecified) expandedHeight else TopAppBarDefaults.LargeAppBarExpandedHeight,
    windowInsets = windowInsets,
    colors = colors,
    scrollBehavior = scrollBehavior,
)

/** Large flexible top app bar (expressive): displaySmall title, optional titleMedium [subtitle]. */
@Composable
fun LargeFlexibleTopAppBar(
    title: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    subtitle: (@Composable () -> Unit)? = null,
    navigationIcon: @Composable () -> Unit = {},
    actions: @Composable RowScope.() -> Unit = {},
    titleHorizontalAlignment: Alignment.Horizontal = Alignment.Start,
    collapsedHeight: Dp = TopAppBarDefaults.LargeAppBarCollapsedHeight,
    expandedHeight: Dp = if (subtitle != null) {
        TopAppBarDefaults.LargeFlexibleAppBarWithSubtitleExpandedHeight
    } else {
        TopAppBarDefaults.LargeFlexibleAppBarWithoutSubtitleExpandedHeight
    },
    windowInsets: WindowInsets = TopAppBarDefaults.windowInsets,
    colors: TopAppBarColors = TopAppBarDefaults.topAppBarColors(),
    scrollBehavior: TopAppBarScrollBehavior? = null,
): Unit = TwoRowsTopAppBar(
    title = title,
    titleTextStyle = MaterialTheme.typography.largeFlexibleTitle,
    smallTitleTextStyle = MaterialTheme.typography.smallTitle,
    titleBottomPadding = LargeTitleBottomPadding,
    smallTitle = title,
    modifier = modifier,
    subtitle = subtitle ?: {},
    subtitleTextStyle = MaterialTheme.typography.largeFlexibleSubtitle,
    smallSubtitle = subtitle ?: {},
    smallSubtitleTextStyle = MaterialTheme.typography.smallSubtitle,
    titleHorizontalAlignment = titleHorizontalAlignment,
    navigationIcon = navigationIcon,
    actions = actions,
    collapsedHeight = if (collapsedHeight.isFiniteSpecified) collapsedHeight else TopAppBarDefaults.LargeAppBarCollapsedHeight,
    expandedHeight = if (expandedHeight.isFiniteSpecified) {
        expandedHeight
    } else if (subtitle != null) {
        TopAppBarDefaults.LargeFlexibleAppBarWithSubtitleExpandedHeight
    } else {
        TopAppBarDefaults.LargeFlexibleAppBarWithoutSubtitleExpandedHeight
    },
    windowInsets = windowInsets,
    colors = colors,
    scrollBehavior = scrollBehavior,
    hasSubtitle = subtitle != null,
)

/**
 * A two-row top app bar whose [title] and [subtitle] slots know which row they are in (`expanded` is true for the
 * bottom row); defaults to the medium flexible heights.
 */
@Composable
fun TwoRowsTopAppBar(
    title: @Composable (expanded: Boolean) -> Unit,
    modifier: Modifier = Modifier,
    subtitle: (@Composable (expanded: Boolean) -> Unit)? = null,
    navigationIcon: @Composable () -> Unit = {},
    actions: @Composable RowScope.() -> Unit = {},
    titleHorizontalAlignment: Alignment.Horizontal = Alignment.Start,
    collapsedHeight: Dp = Dp.Unspecified,
    expandedHeight: Dp = Dp.Unspecified,
    windowInsets: WindowInsets = TopAppBarDefaults.windowInsets,
    colors: TopAppBarColors = TopAppBarDefaults.topAppBarColors(),
    scrollBehavior: TopAppBarScrollBehavior? = null,
) {
    TwoRowsTopAppBar(
        title = { title(true) },
        titleTextStyle = MaterialTheme.typography.mediumFlexibleTitle,
        smallTitleTextStyle = MaterialTheme.typography.smallTitle,
        titleBottomPadding = 0.dp,
        smallTitle = { title(false) },
        modifier = modifier,
        subtitle = { subtitle?.invoke(true) },
        subtitleTextStyle = MaterialTheme.typography.mediumFlexibleSubtitle,
        smallSubtitle = { subtitle?.invoke(false) },
        smallSubtitleTextStyle = MaterialTheme.typography.smallSubtitle,
        titleHorizontalAlignment = titleHorizontalAlignment,
        navigationIcon = navigationIcon,
        actions = actions,
        collapsedHeight = if (collapsedHeight.isFiniteSpecified) collapsedHeight else TopAppBarDefaults.MediumAppBarCollapsedHeight,
        expandedHeight = if (expandedHeight.isFiniteSpecified) {
            expandedHeight
        } else if (subtitle != null) {
            TopAppBarDefaults.MediumFlexibleAppBarWithSubtitleExpandedHeight
        } else {
            TopAppBarDefaults.MediumFlexibleAppBarWithoutSubtitleExpandedHeight
        },
        windowInsets = windowInsets,
        colors = colors,
        scrollBehavior = scrollBehavior,
        hasSubtitle = subtitle != null,
    )
}

// ---- Bottom app bars ---------------------------------------------------------------------------------------------------

@Deprecated("Maintained for binary compatibility", level = DeprecationLevel.HIDDEN)
@Composable
fun BottomAppBar(
    actions: @Composable RowScope.() -> Unit,
    modifier: Modifier = Modifier,
    floatingActionButton: @Composable (() -> Unit)? = null,
    containerColor: Color = BottomAppBarDefaults.containerColor,
    contentColor: Color = contentColorFor(containerColor),
    tonalElevation: Dp = BottomAppBarDefaults.ContainerElevation,
    contentPadding: PaddingValues = BottomAppBarDefaults.ContentPadding,
    windowInsets: WindowInsets = BottomAppBarDefaults.windowInsets,
): Unit = BottomAppBar(
    actions = actions, modifier = modifier, floatingActionButton = floatingActionButton, containerColor = containerColor,
    contentColor = contentColor, tonalElevation = tonalElevation, contentPadding = contentPadding, windowInsets = windowInsets,
    scrollBehavior = null,
)

/** Bottom app bar with [actions] at the start and an optional [floatingActionButton] at the end (80 dp tall). */
@Composable
fun BottomAppBar(
    actions: @Composable RowScope.() -> Unit,
    modifier: Modifier = Modifier,
    floatingActionButton: @Composable (() -> Unit)? = null,
    containerColor: Color = BottomAppBarDefaults.containerColor,
    contentColor: Color = contentColorFor(containerColor),
    tonalElevation: Dp = BottomAppBarDefaults.ContainerElevation,
    contentPadding: PaddingValues = BottomAppBarDefaults.ContentPadding,
    windowInsets: WindowInsets = BottomAppBarDefaults.windowInsets,
    scrollBehavior: BottomAppBarScrollBehavior? = null,
): Unit = BottomAppBar(
    modifier = modifier,
    containerColor = containerColor,
    contentColor = contentColor,
    tonalElevation = tonalElevation,
    windowInsets = windowInsets,
    contentPadding = contentPadding,
    scrollBehavior = scrollBehavior,
) {
    Row(
        modifier = Modifier.weight(1f),
        horizontalArrangement = Arrangement.Start,
        verticalAlignment = Alignment.CenterVertically,
        content = actions,
    )
    if (floatingActionButton != null) {
        Box(
            Modifier.fillMaxHeight().padding(top = FABVerticalPadding, end = FABHorizontalPadding),
            contentAlignment = Alignment.TopStart,
        ) {
            floatingActionButton()
        }
    }
}

@Deprecated("Maintained for binary compatibility", level = DeprecationLevel.HIDDEN)
@Composable
fun BottomAppBar(
    modifier: Modifier = Modifier,
    containerColor: Color = BottomAppBarDefaults.containerColor,
    contentColor: Color = contentColorFor(containerColor),
    tonalElevation: Dp = BottomAppBarDefaults.ContainerElevation,
    contentPadding: PaddingValues = BottomAppBarDefaults.ContentPadding,
    windowInsets: WindowInsets = BottomAppBarDefaults.windowInsets,
    content: @Composable RowScope.() -> Unit,
): Unit = BottomAppBar(
    modifier = modifier, containerColor = containerColor, contentColor = contentColor, tonalElevation = tonalElevation,
    contentPadding = contentPadding, windowInsets = windowInsets, scrollBehavior = null, content = content,
)

/** Bottom app bar with free [content] in a row (80 dp tall, content centred vertically). */
@Composable
fun BottomAppBar(
    modifier: Modifier = Modifier,
    containerColor: Color = BottomAppBarDefaults.containerColor,
    contentColor: Color = contentColorFor(containerColor),
    tonalElevation: Dp = BottomAppBarDefaults.ContainerElevation,
    contentPadding: PaddingValues = BottomAppBarDefaults.ContentPadding,
    windowInsets: WindowInsets = BottomAppBarDefaults.windowInsets,
    scrollBehavior: BottomAppBarScrollBehavior? = null,
    content: @Composable RowScope.() -> Unit,
) {
    BottomAppBarLayout(
        containerHeight = BottomAppBarContainerHeight,
        horizontalArrangement = Arrangement.Start,
        modifier = modifier,
        containerColor = containerColor,
        contentColor = contentColor,
        tonalElevation = tonalElevation,
        contentPadding = contentPadding,
        windowInsets = windowInsets,
        scrollBehavior = scrollBehavior,
        content = content,
    )
}

/** Flexible bottom app bar (docked toolbar, 64 dp): items spread with [horizontalArrangement]. */
@Composable
fun FlexibleBottomAppBar(
    modifier: Modifier = Modifier,
    containerColor: Color = BottomAppBarDefaults.containerColor,
    contentColor: Color = contentColorFor(containerColor),
    contentPadding: PaddingValues = BottomAppBarDefaults.FlexibleContentPadding,
    horizontalArrangement: Arrangement.Horizontal = BottomAppBarDefaults.FlexibleHorizontalArrangement,
    expandedHeight: Dp = BottomAppBarDefaults.FlexibleBottomAppBarHeight,
    windowInsets: WindowInsets = BottomAppBarDefaults.windowInsets,
    scrollBehavior: BottomAppBarScrollBehavior? = null,
    content: @Composable RowScope.() -> Unit,
) {
    BottomAppBarLayout(
        containerHeight = if (expandedHeight.isFiniteSpecified && expandedHeight > 0.dp) {
            expandedHeight
        } else {
            BottomAppBarDefaults.FlexibleBottomAppBarHeight
        },
        horizontalArrangement = horizontalArrangement,
        modifier = modifier,
        containerColor = containerColor,
        contentColor = contentColor,
        tonalElevation = 0.dp, // AppBarTokens.ContainerElevation
        contentPadding = contentPadding,
        windowInsets = windowInsets,
        scrollBehavior = scrollBehavior,
        content = content,
    )
}

@Composable
private fun BottomAppBarLayout(
    containerHeight: Dp,
    horizontalArrangement: Arrangement.Horizontal,
    modifier: Modifier = Modifier,
    containerColor: Color,
    contentColor: Color,
    tonalElevation: Dp,
    contentPadding: PaddingValues,
    windowInsets: WindowInsets,
    scrollBehavior: BottomAppBarScrollBehavior?,
    content: @Composable RowScope.() -> Unit,
) {
    // Dragging the bar itself resizes it like content scrolling does.
    val appBarDragModifier = if (scrollBehavior != null && !scrollBehavior.isPinned) {
        Modifier.draggable(
            orientation = Orientation.Vertical,
            state = rememberDraggableState { delta -> scrollBehavior.state.heightOffset -= delta },
            onDragStopped = { velocity ->
                settleAppBarBottom(scrollBehavior.state, velocity, scrollBehavior.flingAnimationSpec, scrollBehavior.snapAnimationSpec)
            },
        )
    } else {
        Modifier
    }
    val bar = @Composable { m: Modifier ->
        Surface(color = containerColor, contentColor = contentColor, tonalElevation = tonalElevation, shape = RectangleShape, modifier = m) {
            Row(
                Modifier.fillMaxWidth().windowInsetsPadding(windowInsets).height(containerHeight).padding(contentPadding),
                horizontalArrangement = horizontalArrangement,
                verticalAlignment = Alignment.CenterVertically,
                content = content,
            )
        }
    }
    if (scrollBehavior == null) {
        bar(modifier)
        return
    }
    // The bar keeps its full height and is drawn from the top of a box that shrinks by the height offset (it slides out
    // past the bottom of a Scaffold); the offset can collapse the whole bar, insets included.
    val state = scrollBehavior.state
    val insets = windowInsets.asPaddingValues()
    val fullPx = containerHeight.px() + insets.calculateTopPadding().px() + insets.calculateBottomPadding().px()
    SideEffect { state.heightOffsetLimit = -fullPx.toFloat() }
    val visiblePx = (fullPx + state.heightOffset).coerceAtLeast(0f).roundToInt()
    Box(modifier.height(pxDp(visiblePx)).then(appBarDragModifier)) {
        OverflowFrame(boxPx = visiblePx, framePx = fullPx, topPx = 0) { bar(Modifier.fillMaxWidth()) }
    }
}

// ---- Scroll behavior, defaults, state, colors --------------------------------------------------------------------------

/** How a top app bar reacts to the scrolling of the content it is connected to through [nestedScrollConnection]. */
@Stable
interface TopAppBarScrollBehavior {
    val state: TopAppBarState
    val isPinned: Boolean
    val snapAnimationSpec: AnimationSpec<Float>?
    val flingAnimationSpec: DecayAnimationSpec<Float>?
    val nestedScrollConnection: NestedScrollConnection
}

object TopAppBarDefaults {
    @Composable
    fun topAppBarColors(): TopAppBarColors = MaterialTheme.colorScheme.defaultTopAppBarColors

    @Composable
    fun topAppBarColors(
        containerColor: Color = Color.Unspecified,
        scrolledContainerColor: Color = Color.Unspecified,
        navigationIconContentColor: Color = Color.Unspecified,
        titleContentColor: Color = Color.Unspecified,
        actionIconContentColor: Color = Color.Unspecified,
        subtitleContentColor: Color = Color.Unspecified,
    ): TopAppBarColors = MaterialTheme.colorScheme.defaultTopAppBarColors.copy(
        containerColor, scrolledContainerColor, navigationIconContentColor, titleContentColor, actionIconContentColor,
        subtitleContentColor,
    )

    @Deprecated("Maintained for binary compatibility in favor of topAppBarColors with subtitleContentColor", level = DeprecationLevel.HIDDEN)
    @Composable
    fun topAppBarColors(
        containerColor: Color = Color.Unspecified,
        scrolledContainerColor: Color = Color.Unspecified,
        navigationIconContentColor: Color = Color.Unspecified,
        titleContentColor: Color = Color.Unspecified,
        actionIconContentColor: Color = Color.Unspecified,
    ): TopAppBarColors = topAppBarColors(
        containerColor = containerColor,
        scrolledContainerColor = scrolledContainerColor,
        navigationIconContentColor = navigationIconContentColor,
        titleContentColor = titleContentColor,
        actionIconContentColor = actionIconContentColor,
        subtitleContentColor = titleContentColor,
    )

    /** `AppBarTokens`: surface container, surfaceContainer when scrolled, onSurface title, onSurfaceVariant actions. */
    internal val ColorScheme.defaultTopAppBarColors: TopAppBarColors
        get() = TopAppBarColors(
            containerColor = surface,
            scrolledContainerColor = surfaceContainer,
            navigationIconContentColor = onSurface,
            titleContentColor = onSurface,
            actionIconContentColor = onSurfaceVariant,
            subtitleContentColor = onSurfaceVariant,
        )

    val ContentPadding: PaddingValues = PaddingValues(all = 0.dp)

    val windowInsets: WindowInsets
        @Composable get() = WindowInsets.systemBars.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Top)

    val snapAnimationSpec: AnimationSpec<Float>
        @Composable get() = DefaultEffectsFloat

    val flingAnimationSpec: DecayAnimationSpec<Float>
        @Composable get() = rememberSplineBasedDecay()

    @Deprecated("Use topAppBarColors instead", replaceWith = ReplaceWith("topAppBarColors()"), DeprecationLevel.WARNING)
    @Composable
    fun centerAlignedTopAppBarColors(): TopAppBarColors = MaterialTheme.colorScheme.defaultTopAppBarColors

    @Deprecated(
        "Use topAppBarColors instead",
        replaceWith = ReplaceWith(
            "topAppBarColors(containerColor, scrolledContainerColor, navigationIconContentColor, titleContentColor, actionIconContentColor)",
        ),
        DeprecationLevel.WARNING,
    )
    @Composable
    fun centerAlignedTopAppBarColors(
        containerColor: Color = Color.Unspecified,
        scrolledContainerColor: Color = Color.Unspecified,
        navigationIconContentColor: Color = Color.Unspecified,
        titleContentColor: Color = Color.Unspecified,
        actionIconContentColor: Color = Color.Unspecified,
    ): TopAppBarColors = MaterialTheme.colorScheme.defaultTopAppBarColors.copy(
        containerColor, scrolledContainerColor, navigationIconContentColor, titleContentColor, actionIconContentColor,
    )

    @Deprecated("Use topAppBarColors instead", replaceWith = ReplaceWith("topAppBarColors()"), DeprecationLevel.WARNING)
    @Composable
    fun mediumTopAppBarColors(): TopAppBarColors = MaterialTheme.colorScheme.defaultTopAppBarColors

    @Deprecated(
        "Use topAppBarColors instead",
        replaceWith = ReplaceWith(
            "topAppBarColors(containerColor, scrolledContainerColor, navigationIconContentColor, titleContentColor, actionIconContentColor)",
        ),
        DeprecationLevel.WARNING,
    )
    @Composable
    fun mediumTopAppBarColors(
        containerColor: Color = Color.Unspecified,
        scrolledContainerColor: Color = Color.Unspecified,
        navigationIconContentColor: Color = Color.Unspecified,
        titleContentColor: Color = Color.Unspecified,
        actionIconContentColor: Color = Color.Unspecified,
    ): TopAppBarColors = MaterialTheme.colorScheme.defaultTopAppBarColors.copy(
        containerColor, scrolledContainerColor, navigationIconContentColor, titleContentColor, actionIconContentColor,
    )

    @Deprecated("Use topAppBarColors instead", replaceWith = ReplaceWith("topAppBarColors()"), DeprecationLevel.WARNING)
    @Composable
    fun largeTopAppBarColors(): TopAppBarColors = MaterialTheme.colorScheme.defaultTopAppBarColors

    @Deprecated(
        "Use topAppBarColors instead",
        replaceWith = ReplaceWith(
            "topAppBarColors(containerColor, scrolledContainerColor, navigationIconContentColor, titleContentColor, actionIconContentColor)",
        ),
        DeprecationLevel.WARNING,
    )
    @Composable
    fun largeTopAppBarColors(
        containerColor: Color = Color.Unspecified,
        scrolledContainerColor: Color = Color.Unspecified,
        navigationIconContentColor: Color = Color.Unspecified,
        titleContentColor: Color = Color.Unspecified,
        actionIconContentColor: Color = Color.Unspecified,
    ): TopAppBarColors = MaterialTheme.colorScheme.defaultTopAppBarColors.copy(
        containerColor, scrolledContainerColor, navigationIconContentColor, titleContentColor, actionIconContentColor,
    )

    /** The bar stays put; scrolled content only switches its container to the scrolled color. */
    @Composable
    fun pinnedScrollBehavior(
        state: TopAppBarState = rememberTopAppBarState(),
        canScroll: () -> Boolean = { true },
    ): TopAppBarScrollBehavior = remember(state, canScroll) { PinnedScrollBehavior(state = state, canScroll = canScroll) }

    /** [pinnedScrollBehavior] whose scrolled color follows whether [scrollableState] is scrolled away from its start. */
    @Composable
    fun pinnedScrollBehavior(
        scrollableState: ScrollableState,
        state: TopAppBarState = rememberTopAppBarState(),
        canScroll: () -> Boolean = { true },
    ): TopAppBarScrollBehavior = remember(scrollableState, state, canScroll) {
        PinnedScrollBehavior(state = state, canScroll = canScroll, isScrollingContentAtStart = { scrollableState.isAtStart() })
    }

    /** The bar collapses as soon as the content scrolls up and expands as soon as it scrolls down. */
    @Composable
    fun enterAlwaysScrollBehavior(
        state: TopAppBarState = rememberTopAppBarState(),
        canScroll: () -> Boolean = { true },
        snapAnimationSpec: AnimationSpec<Float>? = TopAppBarDefaults.snapAnimationSpec,
        flingAnimationSpec: DecayAnimationSpec<Float>? = TopAppBarDefaults.flingAnimationSpec,
    ): TopAppBarScrollBehavior = remember(state, canScroll, snapAnimationSpec, flingAnimationSpec) {
        EnterAlwaysScrollBehavior(state = state, snapAnimationSpec = snapAnimationSpec, flingAnimationSpec = flingAnimationSpec, canScroll = canScroll)
    }

    @Deprecated(
        message = "Please use the enterAlwaysScrollBehavior() function that takes a scrollableState parameter.",
        replaceWith = ReplaceWith("enterAlwaysScrollBehavior(scrollableState, state, canScroll, snapAnimationSpec, flingAnimationSpec)"),
        level = DeprecationLevel.WARNING,
    )
    @Composable
    fun enterAlwaysScrollBehavior(
        state: TopAppBarState = rememberTopAppBarState(),
        canScroll: () -> Boolean = { true },
        snapAnimationSpec: AnimationSpec<Float>? = TopAppBarDefaults.snapAnimationSpec,
        flingAnimationSpec: DecayAnimationSpec<Float>? = TopAppBarDefaults.flingAnimationSpec,
        reverseLayout: Boolean = false,
    ): TopAppBarScrollBehavior = remember(state, canScroll, snapAnimationSpec, flingAnimationSpec, reverseLayout) {
        LegacyEnterAlwaysScrollBehavior(
            state = state, snapAnimationSpec = snapAnimationSpec, flingAnimationSpec = flingAnimationSpec, canScroll = canScroll,
            reverseLayout = reverseLayout,
        )
    }

    @Deprecated(message = "Please use the enterAlwaysScrollBehavior function that takes a ScrollableState parameter.", level = DeprecationLevel.WARNING)
    @Composable
    fun enterAlwaysScrollBehavior(
        state: TopAppBarState = rememberTopAppBarState(),
        canScroll: () -> Boolean = { true },
        snapAnimationSpec: AnimationSpec<Float>? = TopAppBarDefaults.snapAnimationSpec,
        flingAnimationSpec: DecayAnimationSpec<Float>? = TopAppBarDefaults.flingAnimationSpec,
        isScrollingContentAtStart: () -> Boolean = { true },
    ): TopAppBarScrollBehavior = remember(state, canScroll, snapAnimationSpec, flingAnimationSpec) {
        EnterAlwaysScrollBehavior(
            state = state, snapAnimationSpec = snapAnimationSpec, flingAnimationSpec = flingAnimationSpec, canScroll = canScroll,
            isScrollingContentAtStart = isScrollingContentAtStart,
        )
    }

    /** [enterAlwaysScrollBehavior] whose scrolled color follows whether [scrollableState] is away from its start. */
    @Composable
    fun enterAlwaysScrollBehavior(
        scrollableState: ScrollableState,
        state: TopAppBarState = rememberTopAppBarState(),
        canScroll: () -> Boolean = { true },
        snapAnimationSpec: AnimationSpec<Float>? = TopAppBarDefaults.snapAnimationSpec,
        flingAnimationSpec: DecayAnimationSpec<Float>? = TopAppBarDefaults.flingAnimationSpec,
    ): TopAppBarScrollBehavior = remember(scrollableState, state, canScroll, snapAnimationSpec, flingAnimationSpec) {
        EnterAlwaysScrollBehavior(
            state = state, snapAnimationSpec = snapAnimationSpec, flingAnimationSpec = flingAnimationSpec, canScroll = canScroll,
            isScrollingContentAtStart = { scrollableState.isAtStart() },
        )
    }

    /** The bar collapses as the content scrolls up and expands only once the content is back at its start. */
    @Composable
    fun exitUntilCollapsedScrollBehavior(
        state: TopAppBarState = rememberTopAppBarState(),
        canScroll: () -> Boolean = { true },
        snapAnimationSpec: AnimationSpec<Float>? = TopAppBarDefaults.snapAnimationSpec,
        flingAnimationSpec: DecayAnimationSpec<Float>? = TopAppBarDefaults.flingAnimationSpec,
    ): TopAppBarScrollBehavior = remember(state, canScroll, snapAnimationSpec, flingAnimationSpec) {
        ExitUntilCollapsedScrollBehavior(state = state, snapAnimationSpec = snapAnimationSpec, flingAnimationSpec = flingAnimationSpec, canScroll = canScroll)
    }

    val TopAppBarExpandedHeight: Dp = AppBarSmallContainerHeight
    val MediumAppBarCollapsedHeight: Dp = AppBarSmallContainerHeight
    val MediumAppBarExpandedHeight: Dp = AppBarMediumContainerHeight
    val MediumFlexibleAppBarWithoutSubtitleExpandedHeight: Dp = AppBarMediumFlexibleContainerHeight
    val MediumFlexibleAppBarWithSubtitleExpandedHeight: Dp = AppBarMediumFlexibleLargeContainerHeight
    val LargeAppBarCollapsedHeight: Dp = AppBarSmallContainerHeight
    val LargeAppBarExpandedHeight: Dp = AppBarLargeContainerHeight
    val LargeFlexibleAppBarWithoutSubtitleExpandedHeight: Dp = AppBarLargeFlexibleContainerHeight
    val LargeFlexibleAppBarWithSubtitleExpandedHeight: Dp = AppBarLargeFlexibleLargeContainerHeight
}

/**
 * Whether the content is at its start, as AndroidX reads it from `scrollIndicatorState.scrollOffset == 0`.
 * OneSim's `ScrollableState` has no scroll indicator state; `canScrollBackward` says the same for scroll and lazy states.
 */
private fun ScrollableState.isAtStart(): Boolean = !canScrollBackward

@Composable
fun rememberTopAppBarState(
    initialHeightOffsetLimit: Float = -Float.MAX_VALUE,
    initialHeightOffset: Float = 0f,
    initialContentOffset: Float = 0f,
): TopAppBarState = rememberSaveable(saver = TopAppBarState.Saver) {
    TopAppBarState(initialHeightOffsetLimit, initialHeightOffset, initialContentOffset)
}

/**
 * Scroll state of a top app bar: [heightOffset] (pixels, `heightOffsetLimit..0`) collapses it, [contentOffset] is
 * the content's total scroll, and the fractions drive the container color and the title crossfade.
 */
@Stable
class TopAppBarState(initialHeightOffsetLimit: Float, initialHeightOffset: Float, initialContentOffset: Float) {
    /** The lowest [heightOffset] (negative pixels); the bar sets it to minus its collapsible height when laid out. */
    var heightOffsetLimit: Float = initialHeightOffsetLimit

    var heightOffset: Float
        get() = _heightOffset.floatValue
        set(newOffset) {
            _heightOffset.floatValue = newOffset.coerceIn(minimumValue = heightOffsetLimit, maximumValue = 0f)
        }

    var contentOffset: Float by mutableFloatStateOf(initialContentOffset)

    internal var isScrollingContentAtStart: () -> Boolean = { true }

    val collapsedFraction: Float
        get() = if (heightOffsetLimit != 0f) heightOffset / heightOffsetLimit else 0f

    val overlappedFraction: Float
        get() = if (!isScrollingContentAtStart() && contentOffset == 0f) {
            1f
        } else if (heightOffsetLimit != 0f) {
            1 - ((heightOffsetLimit + contentOffset.absoluteValue).coerceIn(minimumValue = heightOffsetLimit, maximumValue = 0f) / heightOffsetLimit)
        } else {
            0f
        }

    companion object {
        val Saver: Saver<TopAppBarState, *> = listSaver(
            save = { listOf(it.heightOffsetLimit, it.heightOffset, it.contentOffset) },
            restore = { TopAppBarState(initialHeightOffsetLimit = it[0], initialHeightOffset = it[1], initialContentOffset = it[2]) },
        )
    }

    private var _heightOffset = mutableFloatStateOf(initialHeightOffset)
}

@Stable
class TopAppBarColors(
    val containerColor: Color,
    val scrolledContainerColor: Color,
    val navigationIconContentColor: Color,
    val titleContentColor: Color,
    val actionIconContentColor: Color,
    val subtitleContentColor: Color,
) {
    @Deprecated(
        "Use the TopAppBarColors constructor with subtitleContentColor",
        replaceWith = ReplaceWith(
            "TopAppBarColors(containerColor, scrolledContainerColor,navigationIconContentColor, titleContentColor, actionIconContentColor, subtitleContentColor)",
        ),
        DeprecationLevel.WARNING,
    )
    constructor(
        containerColor: Color,
        scrolledContainerColor: Color,
        navigationIconContentColor: Color,
        titleContentColor: Color,
        actionIconContentColor: Color,
    ) : this(containerColor, scrolledContainerColor, navigationIconContentColor, titleContentColor, actionIconContentColor, titleContentColor)

    /** A copy; unspecified colors keep this one's. */
    fun copy(
        containerColor: Color = this.containerColor,
        scrolledContainerColor: Color = this.scrolledContainerColor,
        navigationIconContentColor: Color = this.navigationIconContentColor,
        titleContentColor: Color = this.titleContentColor,
        actionIconContentColor: Color = this.actionIconContentColor,
        subtitleContentColor: Color = this.subtitleContentColor,
    ): TopAppBarColors = TopAppBarColors(
        containerColor.takeOrElse { this.containerColor },
        scrolledContainerColor.takeOrElse { this.scrolledContainerColor },
        navigationIconContentColor.takeOrElse { this.navigationIconContentColor },
        titleContentColor.takeOrElse { this.titleContentColor },
        actionIconContentColor.takeOrElse { this.actionIconContentColor },
        subtitleContentColor.takeOrElse { this.subtitleContentColor },
    )

    /** The container color [colorTransitionFraction] (0...1, FastOutLinearIn) of the way to [scrolledContainerColor]. */
    @Stable
    internal fun containerColor(colorTransitionFraction: Float): Color =
        lerpOklab(containerColor, scrolledContainerColor, FastOutLinearInEasing.transform(colorTransitionFraction))

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is TopAppBarColors) return false
        return containerColor == other.containerColor && scrolledContainerColor == other.scrolledContainerColor &&
            navigationIconContentColor == other.navigationIconContentColor && titleContentColor == other.titleContentColor &&
            actionIconContentColor == other.actionIconContentColor && subtitleContentColor == other.subtitleContentColor
    }

    override fun hashCode(): Int {
        var result = containerColor.hashCode()
        result = 31 * result + scrolledContainerColor.hashCode()
        result = 31 * result + navigationIconContentColor.hashCode()
        result = 31 * result + titleContentColor.hashCode()
        result = 31 * result + actionIconContentColor.hashCode()
        result = 31 * result + subtitleContentColor.hashCode()
        return result
    }
}

/** How a bottom app bar reacts to the scrolling of the content it is connected to through [nestedScrollConnection]. */
@Stable
interface BottomAppBarScrollBehavior {
    val state: BottomAppBarState
    val isPinned: Boolean
    val snapAnimationSpec: AnimationSpec<Float>?
    val flingAnimationSpec: DecayAnimationSpec<Float>?
    val nestedScrollConnection: NestedScrollConnection
}

object BottomAppBarDefaults {
    /** `BottomAppBarTokens.ContainerColor`: surfaceContainer. */
    val containerColor: Color
        @Composable get() = MaterialTheme.colorScheme.surfaceContainer

    val ContainerElevation: Dp = 0.dp

    val ContentPadding: PaddingValues = PaddingValues(
        start = BottomAppBarHorizontalPadding,
        top = BottomAppBarVerticalPadding,
        end = BottomAppBarHorizontalPadding,
    )

    val windowInsets: WindowInsets
        @Composable get() = WindowInsets.systemBars.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom)

    /** `FabSecondaryContainerTokens.ContainerColor`: secondaryContainer. */
    val bottomAppBarFabColor: Color
        @Composable get() = MaterialTheme.colorScheme.secondaryContainer

    val FlexibleContentPadding: PaddingValues = PaddingValues(start = DockedToolbarLeadingSpace, end = DockedToolbarTrailingSpace)

    val FlexibleBottomAppBarHeight: Dp = DockedToolbarContainerHeight

    val FlexibleHorizontalArrangement: Arrangement.Horizontal = Arrangement.SpaceBetween

    val FlexibleFixedHorizontalArrangement: Arrangement.Horizontal =
        Arrangement.spacedBy(DockedToolbarMaxSpacing, Alignment.CenterHorizontally)

    val snapAnimationSpec: AnimationSpec<Float>
        @Composable get() = FastSpatialFloat

    val flingAnimationSpec: DecayAnimationSpec<Float>
        @Composable get() = rememberSplineBasedDecay()

    /** The bar slides out as the content scrolls up and back as soon as it scrolls down. */
    @Composable
    fun exitAlwaysScrollBehavior(
        state: BottomAppBarState = rememberBottomAppBarState(),
        canScroll: () -> Boolean = { true },
        snapAnimationSpec: AnimationSpec<Float>? = BottomAppBarDefaults.snapAnimationSpec,
        flingAnimationSpec: DecayAnimationSpec<Float>? = BottomAppBarDefaults.flingAnimationSpec,
    ): BottomAppBarScrollBehavior = remember(state, canScroll, snapAnimationSpec, flingAnimationSpec) {
        ExitAlwaysScrollBehavior(state = state, snapAnimationSpec = snapAnimationSpec, flingAnimationSpec = flingAnimationSpec, canScroll = canScroll)
    }
}

@Composable
fun rememberBottomAppBarState(
    initialHeightOffsetLimit: Float = -Float.MAX_VALUE,
    initialHeightOffset: Float = 0f,
    initialContentOffset: Float = 0f,
): BottomAppBarState = rememberSaveable(saver = BottomAppBarState.Saver) {
    BottomAppBarState(initialHeightOffsetLimit, initialHeightOffset, initialContentOffset)
}

/** Scroll state of a bottom app bar ([heightOffset] in pixels, `heightOffsetLimit..0`). */
@Stable
interface BottomAppBarState {
    var heightOffsetLimit: Float
    var heightOffset: Float
    var contentOffset: Float
    val collapsedFraction: Float

    companion object {
        val Saver: Saver<BottomAppBarState, *> = listSaver(
            save = { listOf(it.heightOffsetLimit, it.heightOffset, it.contentOffset) },
            restore = { BottomAppBarState(initialHeightOffsetLimit = it[0], initialHeightOffset = it[1], initialContentOffset = it[2]) },
        )
    }
}

fun BottomAppBarState(
    initialHeightOffsetLimit: Float = -Float.MAX_VALUE,
    initialHeightOffset: Float = 0f,
    initialContentOffset: Float = 0f,
): BottomAppBarState = BottomAppBarStateImpl(initialHeightOffsetLimit, initialHeightOffset, initialContentOffset)

@Stable
private class BottomAppBarStateImpl(initialHeightOffsetLimit: Float, initialHeightOffset: Float, initialContentOffset: Float) : BottomAppBarState {
    override var heightOffsetLimit by mutableFloatStateOf(initialHeightOffsetLimit)

    override var heightOffset: Float
        get() = _heightOffset.floatValue
        set(newOffset) {
            _heightOffset.floatValue = newOffset.coerceIn(minimumValue = heightOffsetLimit, maximumValue = 0f)
        }

    override var contentOffset by mutableFloatStateOf(initialContentOffset)

    override val collapsedFraction: Float
        get() = if (heightOffsetLimit != 0f) heightOffset / heightOffsetLimit else 0f

    private var _heightOffset = mutableFloatStateOf(initialHeightOffset)
}

private class ExitAlwaysScrollBehavior(
    override val state: BottomAppBarState,
    override val snapAnimationSpec: AnimationSpec<Float>?,
    override val flingAnimationSpec: DecayAnimationSpec<Float>?,
    val canScroll: () -> Boolean = { true },
) : BottomAppBarScrollBehavior {
    override val isPinned: Boolean = false
    override var nestedScrollConnection = object : NestedScrollConnection {
        override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
            if (!canScroll()) return Offset.Zero
            state.contentOffset += consumed.y
            state.heightOffset = state.heightOffset + consumed.y
            return Offset.Zero
        }

        override suspend fun onPostFling(consumed: Velocity, available: Velocity): Velocity {
            if (available.y > 0f && (state.heightOffset == 0f || state.heightOffset == state.heightOffsetLimit)) {
                // Reset the total content offset to zero when scrolling all the way down (float precision).
                state.contentOffset = 0f
            }
            val superConsumed = super.onPostFling(consumed, available)
            return superConsumed + settleAppBarBottom(state, available.y, flingAnimationSpec, snapAnimationSpec)
        }
    }
}

private suspend fun settleAppBarBottom(
    state: BottomAppBarState,
    velocity: Float,
    flingAnimationSpec: DecayAnimationSpec<Float>?,
    snapAnimationSpec: AnimationSpec<Float>?,
): Velocity = settle(
    { state.heightOffset }, { state.heightOffset = it }, { state.heightOffsetLimit }, { state.collapsedFraction },
    velocity, flingAnimationSpec, snapAnimationSpec,
)

private class PinnedScrollBehavior(
    override val state: TopAppBarState,
    val canScroll: () -> Boolean = { true },
    isScrollingContentAtStart: (() -> Boolean)? = null,
) : TopAppBarScrollBehavior {
    init {
        if (isScrollingContentAtStart != null) state.isScrollingContentAtStart = isScrollingContentAtStart
    }

    override val isPinned: Boolean = true
    override val snapAnimationSpec: AnimationSpec<Float>? = null
    override val flingAnimationSpec: DecayAnimationSpec<Float>? = null
    override var nestedScrollConnection = object : NestedScrollConnection {
        override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
            if (!canScroll()) return Offset.Zero
            state.contentOffset += consumed.y
            return Offset.Zero
        }

        override suspend fun onPostFling(consumed: Velocity, available: Velocity): Velocity {
            if (available.y > 0f) state.contentOffset = 0f
            return super.onPostFling(consumed, available)
        }
    }
}

private class LegacyEnterAlwaysScrollBehavior(
    override val state: TopAppBarState,
    override val snapAnimationSpec: AnimationSpec<Float>?,
    override val flingAnimationSpec: DecayAnimationSpec<Float>?,
    val canScroll: () -> Boolean = { true },
    val reverseLayout: Boolean = false,
) : TopAppBarScrollBehavior {
    override val isPinned: Boolean = false
    override var nestedScrollConnection = object : NestedScrollConnection {
        override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
            if (!canScroll()) return Offset.Zero
            val prevHeightOffset = state.heightOffset
            state.heightOffset += available.y
            // Consumed while the bar is collapsing or expanding (never with a reversed layout).
            return if (!reverseLayout && prevHeightOffset != state.heightOffset) available.copy(x = 0f) else Offset.Zero
        }

        override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
            if (!canScroll()) return Offset.Zero
            state.contentOffset += consumed.y
            if (!reverseLayout) state.heightOffset += consumed.y
            return Offset.Zero
        }

        override suspend fun onPostFling(consumed: Velocity, available: Velocity): Velocity {
            if (available.y > 0f && (state.heightOffset == 0f || state.heightOffset == state.heightOffsetLimit)) {
                state.contentOffset = 0f
            }
            val superConsumed = super.onPostFling(consumed, available)
            return superConsumed + settleAppBar(state, available.y, flingAnimationSpec, snapAnimationSpec)
        }
    }
}

private class EnterAlwaysScrollBehavior(
    override val state: TopAppBarState,
    override val snapAnimationSpec: AnimationSpec<Float>?,
    override val flingAnimationSpec: DecayAnimationSpec<Float>?,
    val canScroll: () -> Boolean = { true },
    isScrollingContentAtStart: (() -> Boolean)? = null,
) : TopAppBarScrollBehavior {
    init {
        if (isScrollingContentAtStart != null) state.isScrollingContentAtStart = isScrollingContentAtStart
    }

    override val isPinned: Boolean = false
    override var nestedScrollConnection = object : NestedScrollConnection {
        override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
            if (!canScroll()) return Offset.Zero
            val prevHeightOffset = state.heightOffset
            state.heightOffset += available.y
            // The offset is coerced into its limits: it changed only while the bar is collapsing or expanding.
            return if (prevHeightOffset != state.heightOffset) available.copy(x = 0f) else Offset.Zero
        }

        override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
            if (!canScroll()) return Offset.Zero
            state.contentOffset += consumed.y
            return Offset.Zero
        }

        override suspend fun onPostFling(consumed: Velocity, available: Velocity): Velocity {
            if (available.y > 0f) state.contentOffset = 0f
            val superConsumed = super.onPostFling(consumed, available)
            return superConsumed + settleAppBar(state, available.y, flingAnimationSpec, snapAnimationSpec)
        }
    }
}

private class ExitUntilCollapsedScrollBehavior(
    override val state: TopAppBarState,
    override val snapAnimationSpec: AnimationSpec<Float>?,
    override val flingAnimationSpec: DecayAnimationSpec<Float>?,
    val canScroll: () -> Boolean = { true },
) : TopAppBarScrollBehavior {
    override val isPinned: Boolean = false
    override var nestedScrollConnection = object : NestedScrollConnection {
        override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
            // Don't intercept if scrolling down.
            if (!canScroll() || available.y > 0f) return Offset.Zero
            val prevHeightOffset = state.heightOffset
            state.heightOffset = state.heightOffset + available.y
            return if (prevHeightOffset != state.heightOffset) available.copy(x = 0f) else Offset.Zero
        }

        override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
            if (!canScroll()) return Offset.Zero
            state.contentOffset += consumed.y
            if (available.y < 0f || consumed.y < 0f) {
                // When scrolling up, just update the state's height offset.
                val oldHeightOffset = state.heightOffset
                state.heightOffset = state.heightOffset + consumed.y
                return Offset(0f, state.heightOffset - oldHeightOffset)
            }
            if (available.y > 0f) {
                // Expand by what the content left over (it is back at its start).
                val oldHeightOffset = state.heightOffset
                state.heightOffset = state.heightOffset + available.y
                return Offset(0f, state.heightOffset - oldHeightOffset)
            }
            return Offset.Zero
        }

        override suspend fun onPostFling(consumed: Velocity, available: Velocity): Velocity {
            if (available.y > 0) state.contentOffset = 0f
            val superConsumed = super.onPostFling(consumed, available)
            return superConsumed + settleAppBar(state, available.y, flingAnimationSpec, snapAnimationSpec)
        }
    }
}

private suspend fun settleAppBar(
    state: TopAppBarState,
    velocity: Float,
    flingAnimationSpec: DecayAnimationSpec<Float>?,
    snapAnimationSpec: AnimationSpec<Float>?,
): Velocity = settle(
    { state.heightOffset }, { state.heightOffset = it }, { state.heightOffsetLimit }, { state.collapsedFraction },
    velocity, flingAnimationSpec, snapAnimationSpec,
)

/**
 * After a fling (or a drag of the bar): continue the motion with [flingAnimationSpec] from [velocity] while the bar
 * takes it, then snap fully open or closed (whichever is nearer) with [snapAnimationSpec]. Returns the velocity left.
 */
private suspend fun settle(
    offset: () -> Float,
    setOffset: (Float) -> Unit,
    limit: () -> Float,
    collapsedFraction: () -> Float,
    velocity: Float,
    flingAnimationSpec: DecayAnimationSpec<Float>?,
    snapAnimationSpec: AnimationSpec<Float>?,
): Velocity {
    // Fully collapsed or expanded: nothing to settle (not checking 0f because of float precision).
    if (collapsedFraction() < 0.01f || collapsedFraction() == 1f) return Velocity.Zero
    var remainingVelocity = velocity
    if (flingAnimationSpec != null && abs(velocity) > 1f) {
        // `AnimationState(0f, velocity).animateDecay(flingAnimationSpec)`, cancelled once the bar stops taking it.
        var lastValue = 0f
        val duration = flingAnimationSpec.durationNanos(velocity)
        var start = -1L
        while (true) {
            val now = withFrameNanos { it }
            if (start < 0) start = now
            val playTime = (now - start).coerceAtMost(duration)
            val value = flingAnimationSpec.valueChange(playTime, velocity)
            val delta = value - lastValue
            val initialHeightOffset = offset()
            setOffset(initialHeightOffset + delta)
            val consumed = abs(initialHeightOffset - offset())
            lastValue = value
            remainingVelocity = if (playTime >= duration) 0f else flingAnimationSpec.velocityAt(playTime, velocity)
            // Avoid rounding errors and stop if anything is unconsumed.
            if (abs(delta - consumed) > 0.5f || playTime >= duration) break
        }
    }
    if (snapAnimationSpec != null) {
        if (offset() < 0 && offset() > limit()) {
            val target = if (collapsedFraction() < 0.5f) 0f else limit()
            Animatable(offset()).animateTo(target, animationSpec = snapAnimationSpec) { setOffset(value) }
        }
    }
    return Velocity(0f, remainingVelocity)
}

// ---- Layout --------------------------------------------------------------------------------------------------------------

@Composable
private fun SingleRowTopAppBar(
    modifier: Modifier = Modifier,
    title: @Composable () -> Unit,
    titleTextStyle: TextStyle,
    subtitle: (@Composable () -> Unit)?,
    subtitleTextStyle: TextStyle,
    titleHorizontalAlignment: Alignment.Horizontal,
    navigationIcon: @Composable () -> Unit,
    actions: @Composable RowScope.() -> Unit,
    expandedHeight: Dp,
    contentPadding: PaddingValues,
    windowInsets: WindowInsets,
    colors: TopAppBarColors,
    scrollBehavior: TopAppBarScrollBehavior?,
) {
    require(expandedHeight.isFiniteSpecified) { "The expandedHeight is expected to be specified and finite" }

    // The container switches to the scrolled color once content scrolls under the bar (animated).
    val targetColor by remember(colors, scrollBehavior) {
        derivedStateOf {
            val overlappingFraction = scrollBehavior?.state?.overlappedFraction ?: 0f
            colors.containerColor(if (overlappingFraction > 0.01f) 1f else 0f)
        }
    }
    val appBarContainerColor by animateColorAsState(targetColor, animationSpec = DefaultEffectsColor)

    Box(modifier.then(appBarDragModifier(scrollBehavior)).background(appBarContainerColor)) {
        TopAppBarLayout(
            modifier = Modifier.windowInsetsPadding(windowInsets),
            scrolledOffset = scrollBehavior?.state?.heightOffset ?: 0f,
            heightOffsetLimitOf = scrollBehavior?.state,
            navigationIconContentColor = colors.navigationIconContentColor,
            titleContentColor = colors.titleContentColor,
            actionIconContentColor = colors.actionIconContentColor,
            subtitleContentColor = colors.subtitleContentColor,
            title = title,
            titleTextStyle = titleTextStyle,
            subtitle = subtitle,
            subtitleTextStyle = subtitleTextStyle,
            hasSubtitle = subtitle != null,
            titleAlpha = 1f,
            titleBottomArranged = false,
            titleHorizontalAlignment = titleHorizontalAlignment,
            titleBottomPadding = 0.dp,
            navigationIcon = navigationIcon,
            actions = actions,
            height = expandedHeight,
            contentPadding = contentPadding,
        )
    }
}

@Composable
private fun TwoRowsTopAppBar(
    modifier: Modifier = Modifier,
    title: @Composable () -> Unit,
    titleTextStyle: TextStyle,
    titleBottomPadding: Dp,
    smallTitle: @Composable () -> Unit,
    smallTitleTextStyle: TextStyle,
    subtitle: (@Composable () -> Unit)?,
    subtitleTextStyle: TextStyle,
    smallSubtitle: (@Composable () -> Unit)?,
    smallSubtitleTextStyle: TextStyle,
    titleHorizontalAlignment: Alignment.Horizontal,
    navigationIcon: @Composable () -> Unit,
    actions: @Composable RowScope.() -> Unit,
    collapsedHeight: Dp,
    expandedHeight: Dp,
    windowInsets: WindowInsets,
    colors: TopAppBarColors,
    scrollBehavior: TopAppBarScrollBehavior?,
    hasSubtitle: Boolean = false,
) {
    require(collapsedHeight.isFiniteSpecified) { "The collapsedHeight is expected to be specified and finite" }
    require(expandedHeight.isFiniteSpecified) { "The expandedHeight is expected to be specified and finite" }
    require(expandedHeight >= collapsedHeight) { "The expandedHeight is expected to be greater or equal to the collapsedHeight" }

    // The container color and both titles follow the collapsed fraction directly.
    val colorTransitionFraction = scrollBehavior?.state?.collapsedFraction ?: 0f
    val appBarContainerColor = colors.containerColor(colorTransitionFraction)
    val topTitleAlpha = TopTitleAlphaEasing.transform(colorTransitionFraction)
    val bottomTitleAlpha = 1f - colorTransitionFraction

    Box(modifier.then(appBarDragModifier(scrollBehavior)).background(appBarContainerColor)) {
        Column {
            TopAppBarLayout(
                modifier = Modifier.windowInsetsPadding(windowInsets),
                scrolledOffset = 0f,
                heightOffsetLimitOf = null,
                navigationIconContentColor = colors.navigationIconContentColor,
                titleContentColor = colors.titleContentColor,
                actionIconContentColor = colors.actionIconContentColor,
                subtitleContentColor = colors.subtitleContentColor,
                title = smallTitle,
                titleTextStyle = smallTitleTextStyle,
                subtitle = smallSubtitle,
                subtitleTextStyle = smallSubtitleTextStyle,
                hasSubtitle = hasSubtitle,
                titleAlpha = topTitleAlpha,
                titleBottomArranged = false,
                titleHorizontalAlignment = titleHorizontalAlignment,
                titleBottomPadding = 0.dp,
                navigationIcon = navigationIcon,
                actions = actions,
                height = collapsedHeight,
                contentPadding = TopAppBarDefaults.ContentPadding,
            )
            TopAppBarLayout(
                // Only the horizontal sides of the insets: the top inset is above the first row.
                modifier = Modifier.windowInsetsPadding(windowInsets.only(WindowInsetsSides.Horizontal)),
                scrolledOffset = scrollBehavior?.state?.heightOffset ?: 0f,
                heightOffsetLimitOf = scrollBehavior?.state,
                navigationIconContentColor = colors.navigationIconContentColor,
                titleContentColor = colors.titleContentColor,
                actionIconContentColor = colors.actionIconContentColor,
                subtitleContentColor = colors.subtitleContentColor,
                title = title,
                titleTextStyle = titleTextStyle,
                subtitle = subtitle,
                subtitleTextStyle = subtitleTextStyle,
                hasSubtitle = hasSubtitle,
                titleAlpha = bottomTitleAlpha,
                titleBottomArranged = true,
                titleHorizontalAlignment = titleHorizontalAlignment,
                titleBottomPadding = titleBottomPadding,
                navigationIcon = {},
                actions = {},
                height = expandedHeight - collapsedHeight,
                contentPadding = TopAppBarDefaults.ContentPadding,
            )
        }
    }
}

/** Dragging the bar itself resizes it like content scrolling does, then settles it. */
@Composable
private fun appBarDragModifier(scrollBehavior: TopAppBarScrollBehavior?): Modifier =
    if (scrollBehavior != null && !scrollBehavior.isPinned) {
        Modifier.draggable(
            orientation = Orientation.Vertical,
            state = rememberDraggableState { delta -> scrollBehavior.state.heightOffset += delta },
            onDragStopped = { velocity ->
                settleAppBar(scrollBehavior.state, velocity, scrollBehavior.flingAnimationSpec, scrollBehavior.snapAnimationSpec)
            },
        )
    } else {
        Modifier
    }

/**
 * One row of a top app bar (AndroidX `TopAppBarLayout` + `TopAppBarMeasurePolicy`): [height] (or the title's height
 * if taller) plus the vertical [contentPadding], shrunk by [scrolledOffset] and clipped. The navigation icon sits at the
 * start, the actions at the end, both centred on the (shrinking) height; the title starts after
 * `max(12 dp, navigation icon)` (Start), is centred on the full width (CenterHorizontally) or ends before the actions
 * (End), and is centred vertically or, with [titleBottomArranged], has its last baseline [titleBottomPadding] above the
 * bottom.
 */
@Composable
private fun TopAppBarLayout(
    modifier: Modifier,
    scrolledOffset: Float,
    heightOffsetLimitOf: TopAppBarState?,
    navigationIconContentColor: Color,
    titleContentColor: Color,
    subtitleContentColor: Color,
    actionIconContentColor: Color,
    title: @Composable () -> Unit,
    titleTextStyle: TextStyle,
    subtitle: (@Composable () -> Unit)?,
    subtitleTextStyle: TextStyle,
    hasSubtitle: Boolean,
    titleAlpha: Float,
    titleBottomArranged: Boolean,
    titleHorizontalAlignment: Alignment.Horizontal,
    titleBottomPadding: Dp,
    navigationIcon: @Composable () -> Unit,
    actions: @Composable RowScope.() -> Unit,
    height: Dp,
    contentPadding: PaddingValues,
) {
    // Title block height and its last baseline, from the Roboto line metrics of the text styles in effect.
    val inherited = LocalTextStyle.current
    val titleLine = lineBox(inherited.merge(titleTextStyle))
    val subtitleLine = if (subtitle != null && hasSubtitle) lineBox(inherited.merge(subtitleTextStyle)) else null
    val titleHeight = titleLine.height + (subtitleLine?.height ?: 0)
    val titleBelowBaseline = subtitleLine?.belowBaseline ?: titleLine.belowBaseline

    val top = contentPadding.calculateTopPadding().px()
    val bottom = contentPadding.calculateBottomPadding().px()
    val maxLayoutHeight = max(height.px(), titleHeight) + top + bottom
    val heightOffset = if (scrolledOffset.isNaN()) 0 else scrolledOffset.roundToInt()
    val layoutHeight = (maxLayoutHeight + heightOffset).coerceAtLeast(0)
    // `adjustHeightOffsetLimit`: the row can collapse by its whole height.
    if (heightOffsetLimitOf != null) {
        SideEffect { heightOffsetLimitOf.heightOffsetLimit = -maxLayoutHeight.toFloat() }
    }
    val contentHeight = layoutHeight + top - bottom
    val start = contentPadding.calculateStartPadding()
    val end = contentPadding.calculateEndPadding()
    val centered = titleHorizontalAlignment == Alignment.CenterHorizontally
    val titleBlock = @Composable {
        TitleBlock(
            titleContentColor, title, titleTextStyle, subtitleContentColor, subtitle, subtitleTextStyle, titleAlpha,
            titleHorizontalAlignment,
        )
    }

    Box(modifier.fillMaxWidth().height(pxDp(layoutHeight)).clip(RectangleShape)) {
        // Icons (and a vertically centred title): a row of the full height, centred on the shrinking content height.
        val rowHeight = maxLayoutHeight - top - bottom
        OverflowFrame(layoutHeight, rowHeight, (contentHeight - rowHeight).floorDiv(2)) {
            Row(Modifier.fillMaxSize().padding(start = start, end = end), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.widthIn(min = TopAppBarTitleInset)) {
                    Box(Modifier.padding(start = TopAppBarHorizontalPadding)) {
                        CompositionLocalProvider(LocalContentColor provides navigationIconContentColor, content = navigationIcon)
                    }
                }
                Box(
                    Modifier.weight(1f),
                    contentAlignment = if (titleHorizontalAlignment == Alignment.End) Alignment.CenterEnd else Alignment.CenterStart,
                ) {
                    if (!titleBottomArranged && !centered) titleBlock()
                }
                Box(Modifier.padding(end = TopAppBarHorizontalPadding)) {
                    CompositionLocalProvider(LocalContentColor provides actionIconContentColor) {
                        Row(horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically, content = actions)
                    }
                }
            }
            if (!titleBottomArranged && centered) {
                // Centred on the whole bar width (not between the icons).
                Box(Modifier.fillMaxSize().padding(horizontal = TopAppBarTitleInset), contentAlignment = Alignment.Center) { titleBlock() }
            }
        }
        if (titleBottomArranged) {
            // The title's last baseline [titleBottomPadding] above the bottom, less if the row has no room for it.
            val titleY = if (titleBottomPadding.px() == 0) {
                contentHeight - titleHeight
            } else {
                val paddingFromBottom = titleBottomPadding.px() - titleBelowBaseline
                val heightWithPadding = paddingFromBottom + titleHeight
                val adjustedBottomPadding = if (heightWithPadding > maxLayoutHeight) {
                    paddingFromBottom - (heightWithPadding - maxLayoutHeight)
                } else {
                    paddingFromBottom
                }
                contentHeight - titleHeight - max(0, adjustedBottomPadding)
            }
            OverflowFrame(layoutHeight, maxLayoutHeight, titleY) {
                Row(Modifier.fillMaxSize().padding(start = start, end = end)) {
                    Box(Modifier.widthIn(min = TopAppBarTitleInset).padding(start = TopAppBarHorizontalPadding))
                    Box(
                        Modifier.weight(1f),
                        contentAlignment = if (titleHorizontalAlignment == Alignment.End) Alignment.TopEnd else Alignment.TopStart,
                    ) {
                        if (!centered) titleBlock()
                    }
                    Box(Modifier.padding(end = TopAppBarHorizontalPadding))
                }
                if (centered) {
                    Box(Modifier.fillMaxSize().padding(horizontal = TopAppBarTitleInset), contentAlignment = Alignment.TopCenter) { titleBlock() }
                }
            }
        }
    }
}

@Composable
private fun TitleBlock(
    titleContentColor: Color,
    title: @Composable () -> Unit,
    titleTextStyle: TextStyle,
    subtitleContentColor: Color,
    subtitle: (@Composable () -> Unit)?,
    subtitleTextStyle: TextStyle,
    titleAlpha: Float,
    titleHorizontalAlignment: Alignment.Horizontal,
) {
    val fade = if (titleAlpha < 1f) Modifier.alpha(titleAlpha.coerceAtLeast(0f)) else Modifier
    if (subtitle != null) {
        Column(Modifier.padding(horizontal = TopAppBarHorizontalPadding).then(fade), horizontalAlignment = titleHorizontalAlignment) {
            ProvideContentColorTextStyle(titleContentColor, titleTextStyle, title)
            ProvideContentColorTextStyle(subtitleContentColor, subtitleTextStyle, subtitle)
        }
    } else {
        Box(Modifier.padding(horizontal = TopAppBarHorizontalPadding).then(fade)) {
            ProvideContentColorTextStyle(titleContentColor, titleTextStyle, title)
        }
    }
}

@Composable
private fun ProvideContentColorTextStyle(contentColor: Color, textStyle: TextStyle, content: @Composable () -> Unit) {
    val merged = LocalTextStyle.current.merge(textStyle)
    CompositionLocalProvider(LocalContentColor provides contentColor, LocalTextStyle provides merged, content = content)
}

/**
 * A frame [framePx] tall whose top is [topPx] below the top of the enclosing box ([boxPx] tall), laid out at its own
 * height even when the box is shorter (AndroidX's app bar layouts measure their slots against the incoming constraints
 * and place them past their own, clipped, bounds).
 */
@Composable
private fun OverflowFrame(boxPx: Int, framePx: Int, topPx: Int, content: @Composable BoxScope.() -> Unit) {
    // The host centres a required height taller than the box (`requiredHeight`): keep the difference even so that
    // offset is a whole pixel, then move the frame from there.
    val tall = framePx + ((boxPx - framePx) and 1)
    val centred = if (tall > boxPx) (boxPx - tall) / 2 else 0
    Box(Modifier.fillMaxWidth().requiredHeight(pxDp(tall)).offset { IntOffset(0, topPx - centred) }) {
        Box(Modifier.fillMaxWidth().height(pxDp(framePx)), content = content)
    }
}

// ---- Pixels and text metrics ----------------------------------------------------------------------------------------------

/** `Dp.roundToPx()` (half up). */
private fun Dp.px(): Int {
    val v = value * Guest.density
    return if (v.isInfinite()) Int.MAX_VALUE else floor(v + 0.5f).toInt()
}

private fun pxDp(px: Int): Dp = (px / Guest.density).dp

private class LineBox(val height: Int, val belowBaseline: Int)

/**
 * Height and baseline (as distance from the bottom) of one line of [style] on Android, in pixels: Roboto's hhea
 * ascent/descent (1900/500 per 2048 em) rounded out to whole pixels, and with a line height Material's
 * `LineHeightStyle(Center, Trim.None)` (the extra space split evenly, the bottom half rounded up).
 */
private fun lineBox(style: TextStyle): LineBox {
    val d = Guest.density
    val sizeSp = if (style.fontSize.isSpecified && !style.fontSize.isEm) style.fontSize.value else 14f
    val sizePx = sizeSp * d
    val ascent = ceil(sizePx * 1900f / 2048f).toInt()
    val descent = ceil(sizePx * 500f / 2048f).toInt()
    val natural = ascent + descent
    if (!style.lineHeight.isSpecified) return LineBox(natural, descent)
    val lineHeightSp = if (style.lineHeight.isEm) style.lineHeight.value * sizeSp else style.lineHeight.value
    val lineHeight = ceil(lineHeightSp * d).toInt()
    val diff = lineHeight - natural
    val descentDiff = ceil(diff * 0.5f).toInt()
    return LineBox(lineHeight, descent + descentDiff)
}

// ---- Color ------------------------------------------------------------------------------------------------------------------

/** `androidx.compose.ui.graphics.lerp`: interpolates in Oklab. */
private fun lerpOklab(start: Color, stop: Color, fraction: Float): Color {
    if (fraction <= 0f) return start
    if (fraction >= 1f) return stop
    val a = toOklab(start)
    val b = toOklab(stop)
    val l = a[0] + (b[0] - a[0]) * fraction
    val m = a[1] + (b[1] - a[1]) * fraction
    val s = a[2] + (b[2] - a[2]) * fraction
    val alpha = (start.alpha + (stop.alpha - start.alpha) * fraction).coerceIn(0f, 1f)
    return fromOklab(l, m, s, alpha)
}

private fun srgbToLinear(c: Float): Float = if (c <= 0.04045f) c / 12.92f else ((c + 0.055f) / 1.055f).toDouble().pow(2.4).toFloat()

private fun linearToSrgb(c: Float): Float =
    (if (c <= 0.0031308f) c * 12.92f else (1.055 * c.toDouble().pow(1.0 / 2.4) - 0.055).toFloat()).coerceIn(0f, 1f)

private fun toOklab(c: Color): FloatArray {
    val r = srgbToLinear(c.red)
    val g = srgbToLinear(c.green)
    val b = srgbToLinear(c.blue)
    val l = cbrt(0.4122214708f * r + 0.5363325363f * g + 0.0514459929f * b)
    val m = cbrt(0.2119034982f * r + 0.6806995451f * g + 0.1073969566f * b)
    val s = cbrt(0.0883024619f * r + 0.2817188376f * g + 0.6299787005f * b)
    return floatArrayOf(
        0.2104542553f * l + 0.7936177850f * m - 0.0040720468f * s,
        1.9779984951f * l - 2.4285922050f * m + 0.4505937099f * s,
        0.0259040371f * l + 0.7827717662f * m - 0.8086757660f * s,
    )
}

private fun fromOklab(lab: Float, aa: Float, bb: Float, alpha: Float): Color {
    val l = (lab + 0.3963377774f * aa + 0.2158037573f * bb).let { it * it * it }
    val m = (lab - 0.1055613458f * aa - 0.0638541728f * bb).let { it * it * it }
    val s = (lab - 0.0894841775f * aa - 1.2914855480f * bb).let { it * it * it }
    return Color(
        linearToSrgb(4.0767416621f * l - 3.3077115913f * m + 0.2309699292f * s),
        linearToSrgb(-1.2684380046f * l + 2.6097574011f * m - 0.3413193965f * s),
        linearToSrgb(-0.0041960863f * l - 0.7034186147f * m + 1.7076147010f * s),
        alpha,
    )
}
