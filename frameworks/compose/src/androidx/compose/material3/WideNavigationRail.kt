package androidx.compose.material3

import androidx.activity.BackEventCompat
import androidx.activity.compose.BackHandler
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.VectorConverter
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.movableContentOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.takeOrElse
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.takeOrElse
import androidx.compose.ui.unit.coerceAtMost
import kotlin.coroutines.cancellation.CancellationException
import kotlin.math.roundToInt
import kotlinx.coroutines.launch
import onesim.compose.Guest
import onesim.compose.Overlay

// ---- State ------------------------------------------------------------------------------------------------------------

/** Possible values of a [WideNavigationRailState]. */
enum class WideNavigationRailValue {
    /** The rail is collapsed. */
    Collapsed,

    /** The rail is expanded. */
    Expanded,
}

/** State of a [WideNavigationRail] or [ModalWideNavigationRail]: expanded or collapsed, and its animation. */
interface WideNavigationRailState {
    /** Whether the state is currently animating. */
    val isAnimating: Boolean

    /** The value the state is animating to, or the current value when idle. */
    val targetValue: WideNavigationRailValue

    /** The value the state last settled at. */
    val currentValue: WideNavigationRailValue

    /** Expand the rail with animation (suspends until it ends or is interrupted). */
    suspend fun expand()

    /** Collapse the rail with animation. */
    suspend fun collapse()

    /** Toggle between expanded and collapsed with animation. */
    suspend fun toggle()

    /** Set the value without animation. */
    suspend fun snapTo(targetValue: WideNavigationRailValue)
}

/** Create and remember a [WideNavigationRailState] (saved across configuration changes by its target value). */
@Composable
fun rememberWideNavigationRailState(
    initialValue: WideNavigationRailValue = WideNavigationRailValue.Collapsed,
): WideNavigationRailState {
    val animationSpec = remember { NavMotion.defaultSpatial<Float>() }
    return rememberSaveable(saver = WideNavigationRailStateImpl.Saver(animationSpec)) {
        WideNavigationRailStateImpl(initialValue = initialValue, animationSpec = animationSpec)
    }
}

internal val WideNavigationRailValue.isExpanded get() = this == WideNavigationRailValue.Expanded

internal operator fun WideNavigationRailValue.not(): WideNavigationRailValue =
    if (this == WideNavigationRailValue.Collapsed) WideNavigationRailValue.Expanded else WideNavigationRailValue.Collapsed

internal class WideNavigationRailStateImpl(
    var initialValue: WideNavigationRailValue,
    private val animationSpec: AnimationSpec<Float>,
) : WideNavigationRailState {
    private val internalState = Animatable(if (initialValue.isExpanded) Expanded else Collapsed, Float.VectorConverter)
    private val currentVal = derivedStateOf {
        if (internalState.value == Expanded) WideNavigationRailValue.Expanded else WideNavigationRailValue.Collapsed
    }

    override val isAnimating: Boolean get() = internalState.isRunning

    override val targetValue: WideNavigationRailValue
        get() = if (internalState.targetValue == Expanded) WideNavigationRailValue.Expanded else WideNavigationRailValue.Collapsed

    override val currentValue: WideNavigationRailValue get() = currentVal.value

    override suspend fun expand() {
        internalState.animateTo(targetValue = Expanded, animationSpec = animationSpec)
    }

    override suspend fun collapse() {
        internalState.animateTo(targetValue = Collapsed, animationSpec = animationSpec)
    }

    override suspend fun toggle() {
        internalState.animateTo(targetValue = if (targetValue.isExpanded) Collapsed else Expanded, animationSpec = animationSpec)
    }

    override suspend fun snapTo(targetValue: WideNavigationRailValue) {
        internalState.snapTo(if (targetValue.isExpanded) Expanded else Collapsed)
    }

    companion object {
        private const val Collapsed = 0f
        private const val Expanded = 1f

        fun Saver(animationSpec: AnimationSpec<Float>) = Saver<WideNavigationRailState, WideNavigationRailValue>(
            save = { it.targetValue },
            restore = { WideNavigationRailStateImpl(it, animationSpec) },
        )
    }
}

/**
 * The sliding state of a modal rail shown with `hideOnCollapse` (AndroidX `ModalWideNavigationRailState`, an
 * `AnchoredDraggableState` between `-width` and 0): its offset in pixels, dragged and animated.
 */
internal class ModalRailSlideState(initialValue: WideNavigationRailValue, private val animationSpec: AnimationSpec<Float>) {
    /** The rail's width in pixels when known (set when a touch reaches it), else `ExpandedRailMaxWidth`. */
    var widthPx by mutableStateOf(0f)
    private val collapsedOffset get() = -(if (widthPx > 0f) widthPx else ExpandedRailMaxWidth.value * Guest.density)
    val offset = Animatable(if (initialValue.isExpanded) 0f else -(ExpandedRailMaxWidth.value * Guest.density))
    var targetValue by mutableStateOf(initialValue)
        private set
    var currentValue by mutableStateOf(initialValue)
        private set
    val isAnimating: Boolean get() = offset.isRunning

    suspend fun animateTo(value: WideNavigationRailValue) {
        targetValue = value
        offset.animateTo(if (value.isExpanded) 0f else collapsedOffset, animationSpec)
        currentValue = value
    }

    suspend fun dragBy(delta: Float) {
        offset.snapTo((offset.value + delta).coerceIn(collapsedOffset, 0f))
    }

    /** AndroidX anchored settle: past 125 dp/s the fling direction wins, else the nearer anchor (50% threshold). */
    suspend fun settle(velocity: Float) {
        val threshold = 125f * Guest.density
        val target = when {
            velocity > threshold -> WideNavigationRailValue.Expanded
            velocity < -threshold -> WideNavigationRailValue.Collapsed
            offset.value > collapsedOffset * 0.5f -> WideNavigationRailValue.Expanded
            else -> WideNavigationRailValue.Collapsed
        }
        animateTo(target)
    }
}

// ---- Rails --------------------------------------------------------------------------------------------------------------

/**
 * Material 3 wide navigation rail: 96 dp wide collapsed (items with the icon on top), 220...360 dp expanded (items with
 * the icon at the start of the label), animating between the two as [state] changes.
 */
@Composable
fun WideNavigationRail(
    modifier: Modifier = Modifier,
    state: WideNavigationRailState = rememberWideNavigationRailState(),
    shape: Shape = WideNavigationRailDefaults.shape,
    colors: WideNavigationRailColors = WideNavigationRailDefaults.colors(),
    header: @Composable (() -> Unit)? = null,
    windowInsets: WindowInsets = WideNavigationRailDefaults.windowInsets,
    arrangement: Arrangement.Vertical = WideNavigationRailDefaults.arrangement,
    contentPadding: PaddingValues = WideNavigationRailDefaults.ContentPadding,
    content: @Composable () -> Unit,
) {
    WideNavigationRailLayout(
        modifier = modifier,
        isModal = false,
        expanded = state.targetValue.isExpanded,
        containerColor = colors.containerColor,
        contentColor = colors.contentColor,
        shape = shape,
        header = header,
        windowInsets = windowInsets,
        arrangement = arrangement,
        contentPadding = contentPadding,
        content = content,
    )
}

@Deprecated(message = "Deprecated in favor of function with contentPadding parameter", level = DeprecationLevel.HIDDEN)
@Composable
fun WideNavigationRail(
    modifier: Modifier = Modifier,
    state: WideNavigationRailState = rememberWideNavigationRailState(),
    shape: Shape = WideNavigationRailDefaults.shape,
    colors: WideNavigationRailColors = WideNavigationRailDefaults.colors(),
    header: @Composable (() -> Unit)? = null,
    windowInsets: WindowInsets = WideNavigationRailDefaults.windowInsets,
    arrangement: Arrangement.Vertical = WideNavigationRailDefaults.arrangement,
    content: @Composable () -> Unit,
) {
    WideNavigationRail(modifier, state, shape, colors, header, windowInsets, arrangement, WideNavigationRailDefaults.ContentPadding, content)
}

/**
 * The rail body. AndroidX measures the items in a custom layout: the width is the widest item (+ 20 dp) clamped to the
 * animated `[minWidth, widthFullRange]`; here a `widthIn` with those animated bounds around the items does the same.
 */
@Composable
private fun WideNavigationRailLayout(
    modifier: Modifier,
    isModal: Boolean,
    expanded: Boolean,
    containerColor: Color,
    contentColor: Color,
    shape: Shape,
    header: @Composable (() -> Unit)?,
    windowInsets: WindowInsets,
    arrangement: Arrangement.Vertical,
    contentPadding: PaddingValues,
    content: @Composable () -> Unit,
) {
    val minimumA11ySize = LocalMinimumInteractiveComponentSize.current.takeOrElse { 0.dp }
    val animationSpec = remember { NavMotion.defaultSpatial<Dp>() }
    val modalAnimationSpec = remember { NavMotion.fastSpatial<Dp>() }
    val minWidth by animateDpAsState(
        targetValue = if (!expanded) CollapsedRailWidth else ExpandedRailMinWidth,
        animationSpec = if (!isModal) animationSpec else modalAnimationSpec,
    )
    val widthFullRange by animateDpAsState(
        targetValue = if (!expanded) CollapsedRailWidth else ExpandedRailMaxWidth,
        animationSpec = if (!isModal) animationSpec else modalAnimationSpec,
    )
    val itemVerticalSpacedBy by animateDpAsState(if (!expanded) CollapsedItemVerticalSpace else 0.dp, animationSpec)
    val itemMinHeight by animateDpAsState(if (!expanded) TopIconItemMinHeight else minimumA11ySize, animationSpec)

    Surface(color = containerColor, contentColor = contentColor, shape = shape, modifier = modifier) {
        Box(
            Modifier.fillMaxHeight()
                .windowInsetsPadding(windowInsets)
                .widthIn(min = minWidth, max = maxOf(widthFullRange.value, minWidth.value).dp.coerceAtMost(ExpandedRailMaxWidth))
                .selectableGroup(),
        ) {
            CompositionLocalProvider(LocalNavWideRailScope provides NavWideRailScope(itemMinHeight, itemVerticalSpacedBy)) {
                val itemsArrangement = spacedArrangement(arrangement, itemVerticalSpacedBy)
                // Expanded, the rail is 20 dp wider than its widest item (the items start 20 dp in).
                val itemsEnd = if (expanded) WNRItemHorizontalPadding else 0.dp
                if (arrangement == Arrangement.Center) {
                    // Centred items ignore the header and the top/bottom content padding (AndroidX arranges them over
                    // the whole height).
                    if (header != null) Box(Modifier.padding(contentPadding)) { header() }
                    Column(
                        Modifier.fillMaxHeight().padding(
                            start = contentPadding.calculateStartPadding(),
                            end = contentPadding.calculateEndPadding() + itemsEnd,
                        ),
                        verticalArrangement = itemsArrangement,
                    ) { content() }
                } else {
                    Column(Modifier.fillMaxHeight().padding(contentPadding)) {
                        if (header != null) {
                            header()
                            Spacer(Modifier.height(WNRHeaderPadding))
                        }
                        Column(Modifier.weight(1f).padding(end = itemsEnd), verticalArrangement = itemsArrangement) { content() }
                    }
                }
            }
        }
    }
}

/** AndroidX adds the item spacing to each item before arranging; for Top / Center / Bottom that is `spacedBy`. */
private fun spacedArrangement(arrangement: Arrangement.Vertical, space: Dp): Arrangement.Vertical = when (arrangement) {
    Arrangement.Top -> Arrangement.spacedBy(space, androidx.compose.ui.Alignment.Top)
    Arrangement.Center -> Arrangement.spacedBy(space, androidx.compose.ui.Alignment.CenterVertically)
    Arrangement.Bottom -> Arrangement.spacedBy(space, androidx.compose.ui.Alignment.Bottom)
    else -> arrangement
}

/**
 * Material 3 modal wide navigation rail. Collapsed it is a [WideNavigationRail] in place (or nothing, with
 * [hideOnCollapse]); expanded it opens over the screen with a scrim (a tap on it, or system back when
 * [ModalWideNavigationRailProperties.shouldDismissOnBackPress], collapses it). With [hideOnCollapse] it slides in from
 * the start edge and can be dragged closed.
 */
@Composable
fun ModalWideNavigationRail(
    modifier: Modifier = Modifier,
    state: WideNavigationRailState = rememberWideNavigationRailState(),
    hideOnCollapse: Boolean = false,
    collapsedShape: Shape = WideNavigationRailDefaults.modalCollapsedShape,
    expandedShape: Shape = WideNavigationRailDefaults.modalExpandedShape,
    colors: WideNavigationRailColors = WideNavigationRailDefaults.colors(),
    header: @Composable (() -> Unit)? = null,
    expandedHeaderTopPadding: Dp = 0.dp,
    windowInsets: WindowInsets = WideNavigationRailDefaults.windowInsets,
    arrangement: Arrangement.Vertical = WideNavigationRailDefaults.arrangement,
    expandedProperties: ModalWideNavigationRailProperties = WideNavigationRailDefaults.ModalExpandedProperties,
    contentPadding: PaddingValues = WideNavigationRailDefaults.ContentPadding,
    content: @Composable () -> Unit,
) {
    val positionProgress = animateFloatAsState(
        targetValue = if (!state.targetValue.isExpanded) 0f else 1f,
        animationSpec = remember { NavMotion.defaultEffects<Float>() },
    )
    val isCollapsed by remember { derivedStateOf { positionProgress.value == 0f } }
    val modalExpanded by remember { derivedStateOf { positionProgress.value >= 0.3f } }

    val rememberContent = if (hideOnCollapse) content else remember(content) { movableContentOf(content) }
    val modalState = remember(state) { ModalRailSlideState(state.targetValue, NavMotion.defaultSpatial()) }
    val scope = rememberCoroutineScope()
    val animateToDismiss: () -> Unit = {
        scope.launch {
            if (hideOnCollapse) modalState.animateTo(WideNavigationRailValue.Collapsed)
            state.collapse()
        }
    }
    val modalAnimateToDismiss: suspend () -> Unit = {
        if (hideOnCollapse && !modalState.targetValue.isExpanded) state.collapse()
    }

    if (!hideOnCollapse) {
        WideNavigationRailLayout(
            modifier = modifier,
            isModal = false,
            expanded = false,
            containerColor = colors.containerColor,
            contentColor = colors.contentColor,
            shape = collapsedShape,
            header = header,
            windowInsets = windowInsets,
            arrangement = arrangement,
            contentPadding = contentPadding,
            content = { if (isCollapsed) rememberContent() },
        )
    }

    if (hideOnCollapse) {
        // Follow the rail state with the slide (AndroidX feeds it through a conflated channel).
        LaunchedEffect(modalState) {
            snapshotFlow { state.targetValue }.collect { target ->
                if (target != modalState.targetValue) launch { modalState.animateTo(target) }
            }
        }
    }

    if (!isCollapsed) {
        val predictiveBackProgress = remember { Animatable(0f) }
        val swipeEdgeMatchesRail = remember { mutableStateOf(true) }
        Overlay {
            ModalRailBackHandler(
                properties = expandedProperties,
                onDismissRequest = { scope.launch { state.collapse() } },
                onPredictiveBack = { p -> scope.launch { predictiveBackProgress.snapTo(p) } },
                onPredictiveBackCancelled = { scope.launch { predictiveBackProgress.animateTo(0f) } },
                onSwipeEdge = { edge -> swipeEdgeMatchesRail.value = edge == BackEventCompat.EDGE_LEFT },
            )
            Box(Modifier.fillMaxSize()) {
                val isScrimVisible = if (hideOnCollapse) modalState.targetValue != WideNavigationRailValue.Collapsed else modalExpanded
                val alpha by animateFloatAsState(if (isScrimVisible) 1f else 0f, remember { NavMotion.defaultEffects<Float>() })
                RailScrim(color = colors.modalScrimColor, alpha = alpha, onClick = animateToDismiss)
                ModalWideNavigationRailContent(
                    expanded = hideOnCollapse || modalExpanded,
                    isStandaloneModal = hideOnCollapse,
                    predictiveBackProgress = predictiveBackProgress.value,
                    modalAnimateToDismiss = modalAnimateToDismiss,
                    modifier = modifier,
                    railState = modalState,
                    modalContainerColor = colors.modalContainerColor,
                    modalContentColor = colors.modalContentColor,
                    shape = expandedShape,
                    header = if (header != null || expandedHeaderTopPadding.value > 0f) {
                        { Box(Modifier.padding(top = expandedHeaderTopPadding)) { header?.invoke() } }
                    } else {
                        null
                    },
                    windowInsets = windowInsets,
                    gesturesEnabled = hideOnCollapse,
                    arrangement = arrangement,
                    contentPadding = contentPadding,
                    content = rememberContent,
                )
            }
        }
    }
}

@Deprecated(message = "Deprecated in favor of function with contentPadding parameter", level = DeprecationLevel.HIDDEN)
@Composable
fun ModalWideNavigationRail(
    modifier: Modifier = Modifier,
    state: WideNavigationRailState = rememberWideNavigationRailState(),
    hideOnCollapse: Boolean = false,
    collapsedShape: Shape = WideNavigationRailDefaults.modalCollapsedShape,
    expandedShape: Shape = WideNavigationRailDefaults.modalExpandedShape,
    colors: WideNavigationRailColors = WideNavigationRailDefaults.colors(),
    header: @Composable (() -> Unit)? = null,
    expandedHeaderTopPadding: Dp = 0.dp,
    windowInsets: WindowInsets = WideNavigationRailDefaults.windowInsets,
    arrangement: Arrangement.Vertical = WideNavigationRailDefaults.arrangement,
    expandedProperties: ModalWideNavigationRailProperties = WideNavigationRailDefaults.ModalExpandedProperties,
    content: @Composable () -> Unit,
) {
    ModalWideNavigationRail(
        modifier, state, hideOnCollapse, collapsedShape, expandedShape, colors, header, expandedHeaderTopPadding, windowInsets,
        arrangement, expandedProperties, WideNavigationRailDefaults.ContentPadding, content,
    )
}

/** AndroidX `PredictiveBack.transform`. */
private val PredictiveBackEasing = CubicBezierEasing(0.1f, 0.1f, 0f, 1f)

/**
 * The modal rail's window back handling (AndroidX `ModalWideNavigationRailDialog`): while the rail is open, system back
 * goes to it; a back gesture's progress drives the predictive back animation and committing it dismisses the rail if
 * [ModalWideNavigationRailProperties.shouldDismissOnBackPress].
 */
@Composable
private fun ModalRailBackHandler(
    properties: ModalWideNavigationRailProperties,
    onDismissRequest: () -> Unit,
    onPredictiveBack: (Float) -> Unit,
    onPredictiveBackCancelled: () -> Unit,
    onSwipeEdge: (Int) -> Unit,
) {
    if (!properties.shouldDismissOnBackPress) {
        // The window still takes back presses; they do nothing.
        BackHandler(enabled = true) {}
        return
    }
    PredictiveBackHandler(enabled = true) { progress ->
        try {
            progress.collect { e ->
                onSwipeEdge(e.swipeEdge)
                onPredictiveBack(PredictiveBackEasing.transform(e.progress))
            }
            onDismissRequest()
        } catch (e: CancellationException) {
            onPredictiveBackCancelled()
            throw e
        }
    }
}

/** A full-screen scrim; a tap on it calls [onClick]. */
@Composable
private fun RailScrim(color: Color, alpha: Float, onClick: () -> Unit) {
    if (color.isUnspecified) return
    val a = alpha.coerceIn(0f, 1f)
    Box(
        Modifier.fillMaxSize()
            .background(color.copy(alpha = color.alpha * a))
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = onClick),
    )
}

@Composable
private fun ModalWideNavigationRailContent(
    expanded: Boolean,
    isStandaloneModal: Boolean,
    predictiveBackProgress: Float,
    modalAnimateToDismiss: suspend () -> Unit,
    modifier: Modifier,
    railState: ModalRailSlideState,
    modalContainerColor: Color,
    modalContentColor: Color,
    shape: Shape,
    header: @Composable (() -> Unit)?,
    windowInsets: WindowInsets,
    gesturesEnabled: Boolean,
    arrangement: Arrangement.Vertical,
    contentPadding: PaddingValues,
    content: @Composable () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val dragState = rememberDraggableState { delta -> scope.launch { railState.dragBy(delta) } }
    val widthHolder = remember { IntArray(1) }
    // Predictive back shrinks the rail by up to 48 dp in height around its middle (AndroidX scales it; OneSim has no
    // scale transform, so the rail is inset instead).
    val shrink = PredictiveBackMaxScaleYDistance * predictiveBackProgress.coerceIn(0f, 1f) / 2f
    var m = modifier
        .padding(vertical = shrink)
        .widthIn(max = ExpandedRailMaxWidth)
        .fillMaxHeight()
    if (isStandaloneModal) m = m.offset { IntOffset(railState.offset.value.roundToInt(), 0) }
    m = m.navObserveWidth(widthHolder)
        .draggable(
            state = dragState,
            orientation = Orientation.Horizontal,
            enabled = gesturesEnabled,
            onDragStarted = { if (widthHolder[0] > 0) railState.widthPx = widthHolder[0].toFloat() },
            onDragStopped = { velocity ->
                try {
                    railState.settle(velocity)
                } finally {
                    modalAnimateToDismiss()
                }
            },
        )
    WideNavigationRailLayout(
        modifier = m,
        isModal = true,
        expanded = expanded,
        containerColor = modalContainerColor,
        contentColor = modalContentColor,
        shape = shape,
        header = header,
        windowInsets = windowInsets,
        arrangement = arrangement,
        contentPadding = contentPadding,
        content = content,
    )
}

// ---- Item ---------------------------------------------------------------------------------------------------------------

/**
 * An item of a [WideNavigationRail]: in a collapsed rail the icon is above the label on a 56 x 32 dp pill, in an expanded
 * one the icon is at the start of the label on a 56 dp tall pill ([iconPosition]).
 */
@Composable
fun WideNavigationRailItem(
    selected: Boolean,
    onClick: () -> Unit,
    icon: @Composable () -> Unit,
    label: @Composable (() -> Unit)?,
    railExpanded: Boolean,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    iconPosition: NavigationItemIconPosition = WideNavigationRailItemDefaults.iconPositionFor(railExpanded),
    colors: NavigationItemColors = WideNavigationRailItemDefaults.colors(),
    interactionSource: MutableInteractionSource? = null,
    indicatorPadding: PaddingValues = WideNavigationRailItemDefaults.indicatorPadding(railExpanded = railExpanded),
) {
    @Suppress("NAME_SHADOWING")
    val interactionSource = interactionSource ?: remember { MutableInteractionSource() }
    AnimatedNavigationItemImpl(
        selected = selected,
        onClick = onClick,
        icon = icon,
        indicatorShape = androidx.compose.foundation.shape.CircleShape,
        topIconIndicatorWidth = 56.dp,
        topIconLabelTextStyle = MaterialTheme.typography.labelMedium,
        startIconLabelTextStyle = MaterialTheme.typography.labelLarge,
        indicatorPadding = indicatorPadding,
        topIconIndicatorToLabelVerticalPadding = 4.dp,
        noLabelIndicatorPadding = WNRItemNoLabelIndicatorPadding,
        startIconToLabelHorizontalPadding = 8.dp,
        itemHorizontalPadding = WNRItemHorizontalPadding,
        textColor = colors.textColor(selected, enabled, iconPosition == NavigationItemIconPosition.Top),
        iconColor = colors.iconColor(selected, enabled),
        indicatorColor = colors.selectedIndicatorColor,
        modifier = modifier,
        enabled = enabled,
        label = label,
        iconPosition = iconPosition,
        interactionSource = interactionSource,
    )
}

@Deprecated(message = "Deprecated in favor of function with indicatorPadding parameter", level = DeprecationLevel.HIDDEN)
@Composable
fun WideNavigationRailItem(
    selected: Boolean,
    onClick: () -> Unit,
    icon: @Composable () -> Unit,
    label: @Composable (() -> Unit)?,
    railExpanded: Boolean,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    iconPosition: NavigationItemIconPosition = WideNavigationRailItemDefaults.iconPositionFor(railExpanded),
    colors: NavigationItemColors = WideNavigationRailItemDefaults.colors(),
    interactionSource: MutableInteractionSource? = null,
): Unit = WideNavigationRailItem(
    selected, onClick, icon, label, railExpanded, modifier, enabled, iconPosition, colors, interactionSource,
    WideNavigationRailItemDefaults.indicatorPadding(railExpanded = railExpanded),
)

// ---- Colors, properties, defaults ---------------------------------------------------------------------------------------

/** Colors of a [WideNavigationRail] and [ModalWideNavigationRail]. */
@Immutable
class WideNavigationRailColors(
    val containerColor: Color,
    val contentColor: Color,
    val modalContainerColor: Color,
    val modalScrimColor: Color,
    val modalContentColor: Color,
) {
    /** A copy with some colors replaced; [Color.Unspecified] keeps this one's. */
    fun copy(
        containerColor: Color = this.containerColor,
        contentColor: Color = this.contentColor,
        modalContainerColor: Color = this.modalContainerColor,
        modalScrimColor: Color = this.modalScrimColor,
        modalContentColor: Color = this.modalContentColor,
    ): WideNavigationRailColors = WideNavigationRailColors(
        containerColor = containerColor.takeOrElse { this.containerColor },
        contentColor = contentColor.takeOrElse { this.contentColor },
        modalContainerColor = modalContainerColor.takeOrElse { this.modalContainerColor },
        modalScrimColor = modalScrimColor.takeOrElse { this.modalScrimColor },
        modalContentColor = modalContentColor.takeOrElse { this.modalContentColor },
    )

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is WideNavigationRailColors) return false
        return containerColor == other.containerColor && contentColor == other.contentColor &&
            modalContainerColor == other.modalContainerColor && modalScrimColor == other.modalScrimColor
    }

    override fun hashCode(): Int {
        var result = containerColor.hashCode()
        result = 31 * result + contentColor.hashCode()
        result = 31 * result + modalContainerColor.hashCode()
        result = 31 * result + modalScrimColor.hashCode()
        result = 31 * result + modalContentColor.hashCode()
        return result
    }
}

/** Behaviour of an expanded [ModalWideNavigationRail]. */
@Immutable
class ModalWideNavigationRailProperties(
    /** Whether system back (the back key or a back gesture) collapses the rail. */
    val shouldDismissOnBackPress: Boolean = true,
) {
    override fun equals(other: Any?): Boolean =
        this === other || other is ModalWideNavigationRailProperties && shouldDismissOnBackPress == other.shouldDismissOnBackPress

    override fun hashCode(): Int = shouldDismissOnBackPress.hashCode()
}

object WideNavigationRailDefaults {
    /** Default container shape (no rounding). */
    val shape: Shape @Composable get() = androidx.compose.ui.graphics.RectangleShape

    /** Default items arrangement. */
    val arrangement: Arrangement.Vertical get() = Arrangement.Top

    /** Default window insets: the system bars at the top and bottom. */
    val windowInsets: WindowInsets
        @Composable get() = WindowInsets.systemBars.only(WindowInsetsSides.Vertical + WindowInsetsSides.Start)

    /** Shape of a collapsed modal rail. */
    val modalCollapsedShape: Shape @Composable get() = shape

    /** Shape of an expanded modal rail (`shapes.large`, 16 dp corners). */
    val modalExpandedShape: Shape @Composable get() = MaterialTheme.shapes.large

    /** Default properties of an expanded modal rail. */
    val ModalExpandedProperties: ModalWideNavigationRailProperties = ModalWideNavigationRailProperties()

    /** The default [WideNavigationRailColors]. */
    @Composable
    fun colors(): WideNavigationRailColors = defaultColors()

    /** [WideNavigationRailColors] with the given colors. */
    @Composable
    fun colors(
        containerColor: Color = MaterialTheme.colorScheme.surface,
        contentColor: Color = contentColorFor(containerColor),
        modalContainerColor: Color = MaterialTheme.colorScheme.surfaceContainer,
        modalScrimColor: Color = MaterialTheme.colorScheme.scrim.copy(alpha = ScrimOpacity),
        modalContentColor: Color = contentColorFor(modalContainerColor),
    ): WideNavigationRailColors = defaultColors().copy(
        containerColor = containerColor,
        contentColor = contentColor,
        modalContainerColor = modalContainerColor,
        modalScrimColor = modalScrimColor,
        modalContentColor = modalContentColor,
    )

    /** Default content padding: 44 dp above the items. */
    val ContentPadding: PaddingValues = PaddingValues(start = 0.dp, top = WNRTopPadding, end = 0.dp, bottom = 0.dp)

    @Composable
    private fun defaultColors(): WideNavigationRailColors {
        val scheme = MaterialTheme.colorScheme
        return WideNavigationRailColors(
            containerColor = scheme.surface,
            contentColor = contentColorFor(scheme.surface),
            modalContainerColor = scheme.surfaceContainer,
            modalContentColor = contentColorFor(scheme.surfaceContainer),
            modalScrimColor = scheme.scrim.copy(alpha = ScrimOpacity),
        )
    }
}

object WideNavigationRailItemDefaults {
    /**
     * The indicator padding of an item: [collapsedPadding] with the icon on top, [expandedPadding] with the icon at the
     * start, following the item's icon position.
     */
    @Composable
    fun indicatorPadding(
        railExpanded: Boolean,
        collapsedPadding: PaddingValues = IndicatorCollapsedPadding,
        expandedPadding: PaddingValues = IndicatorExpandedPadding,
    ): PaddingValues = remember(collapsedPadding, expandedPadding, railExpanded) {
        NavDynamicPadding.register(if (railExpanded) expandedPadding else collapsedPadding, collapsedPadding, expandedPadding)
    }

    /** [NavigationItemIconPosition.Start] in an expanded rail, else [NavigationItemIconPosition.Top]. */
    fun iconPositionFor(railExpanded: Boolean): NavigationItemIconPosition =
        if (railExpanded) NavigationItemIconPosition.Start else NavigationItemIconPosition.Top

    /** The default [NavigationItemColors] of a [WideNavigationRailItem]. */
    @Composable
    fun colors(): NavigationItemColors = MaterialTheme.colorScheme.defaultWideNavigationRailItemColors()

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
    ): NavigationItemColors = MaterialTheme.colorScheme.defaultWideNavigationRailItemColors().copy(
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

    /** Indicator padding with the icon on top: 16 dp horizontally, 4 dp vertically (a 56 x 32 dp pill). */
    val IndicatorCollapsedPadding: PaddingValues = PaddingValues(horizontal = 16.dp, vertical = 4.dp)

    /** Indicator padding with the icon at the start: 16 dp horizontally, 16 dp vertically (a 56 dp tall pill). */
    val IndicatorExpandedPadding: PaddingValues = PaddingValues(horizontal = 16.dp, vertical = 16.dp)

    private fun ColorScheme.defaultWideNavigationRailItemColors(): NavigationItemColors = NavigationItemColors(
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

/** (56 - 24) / 2. */
internal val WNRItemNoLabelIndicatorPadding: Dp = 16.dp
internal val WNRItemHorizontalPadding: Dp = 20.dp
/** `NavigationRailCollapsedTokens.TopSpace`. */
private val WNRTopPadding: Dp = 44.dp
/** `NavigationRailBaselineItemTokens.HeaderSpaceMinimum`. */
private val WNRHeaderPadding: Dp = 40.dp
private val CollapsedRailWidth: Dp = 96.dp
private val ExpandedRailMinWidth: Dp = 220.dp
private val ExpandedRailMaxWidth: Dp = 360.dp
/** `NavigationRailBaselineItemTokens.ContainerHeight`. */
private val TopIconItemMinHeight: Dp = 64.dp
/** `NavigationRailCollapsedTokens.ItemVerticalSpace`. */
private val CollapsedItemVerticalSpace: Dp = 4.dp
private val PredictiveBackMaxScaleYDistance: Dp = 48.dp
/** `ScrimTokens.ContainerOpacity`. */
private const val ScrimOpacity = 0.32f
