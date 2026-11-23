package androidx.compose.material3

import androidx.activity.BackEventCompat
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.TweenSpec
import androidx.compose.animation.core.sampleSpec
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.spring
import androidx.compose.foundation.MutatePriority
import androidx.compose.foundation.MutatorMutex
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.DragScope
import androidx.compose.foundation.gestures.DraggableState
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.interaction.Interaction
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.WireModifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import kotlin.math.abs
import kotlin.math.roundToInt
import kotlin.math.sign
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import onesim.compose.Guest

// Material 3 navigation drawers (material3 1.5.0-beta01 `NavigationDrawer.kt`), built from Box/Row/Column.
//
// OneSim approximations (the guest cannot measure or transform layers):
// - The sheet width (the closed anchor) is not measured: a drawer sheet reports the width its modifier and
//   `sizeIn(240.dp, 360.dp)` give when its content fills the sheet (`min(360.dp, screen width)` by default); custom
//   drawer content that is not a `*DrawerSheet` is assumed `DrawerDefaults.MaximumDrawerWidth` wide (or the screen).
// - `anchoredDraggable` is reimplemented here (positional threshold 50%, velocity threshold 400 dp/s, closest anchor as
//   the target while dragging); a settle after a drag animates with the source's `tween(256)` snap spec and does not
//   run the decay part of the fling.
// - The predictive back scale (graphicsLayer scaleX/scaleY) is approximated: the sheet shrinks vertically by the
//   same distance (up to 48 dp, centred) with its content moved, not scaled; the 12/24 dp horizontal stretch is not
//   drawn. The bounce overshoot stretch (`horizontalScaleUp`) is approximated by holding the sheet at the edge.
// - Layout direction is always LTR; focus (`focusRequester`), the Escape key and semantics (`paneTitle`, `dismiss`)
//   have no effect.

/** Possible values of [DrawerState]. */
enum class DrawerValue {
    /** The state of the drawer when it is closed. */
    Closed,

    /** The state of the drawer when it is open. */
    Open,
}

/**
 * State of the [ModalNavigationDrawer] and [DismissibleNavigationDrawer] composable.
 *
 * @param initialValue The initial value of the state.
 * @param confirmStateChange Optional callback invoked to confirm or veto a pending state change.
 */
@Suppress("NotCloseable")
@Stable
class DrawerState(
    initialValue: DrawerValue,
    internal val confirmStateChange: (DrawerValue) -> Boolean = { true },
) {
    // ---- anchored draggable (AnchoredDraggableState) ----------------------------------------------------------------

    /** Closed anchor in pixels (open is 0), or NaN before a drawer set the anchors. */
    private var closedAnchor by mutableFloatStateOf(Float.NaN)
    private var offsetState by mutableFloatStateOf(Float.NaN)
    private var settledValue by mutableStateOf(initialValue)
    private var dragTarget by mutableStateOf<DrawerValue?>(null)
    private val mutex = MutatorMutex()
    internal var lastVelocity = 0f
        private set

    private val hasAnchors get() = !closedAnchor.isNaN()

    private fun positionOf(value: DrawerValue) = when (value) {
        DrawerValue.Closed -> closedAnchor
        DrawerValue.Open -> 0f
    }

    private fun closestAnchor(offset: Float): DrawerValue? = when {
        !hasAnchors || offset.isNaN() -> null
        abs(offset - closedAnchor) < abs(offset) -> DrawerValue.Closed
        else -> DrawerValue.Open
    }

    /** The closest anchor in the direction of [searchUpwards] (higher positions), as `closestAnchor(offset, up)`. */
    private fun closestAnchor(offset: Float, searchUpwards: Boolean): DrawerValue {
        val candidates = DrawerValue.entries.filter { if (searchUpwards) positionOf(it) >= offset else positionOf(it) <= offset }
        return candidates.minByOrNull { abs(positionOf(it) - offset) }
            ?: if (searchUpwards) DrawerValue.Open else DrawerValue.Closed
    }

    /** Sets the anchors (closed at [closed] pixels, open at 0); a settled drawer keeps its value at the new position. */
    internal fun updateAnchors(closed: Float) {
        if (closed == closedAnchor) return
        val wasUnset = offsetState.isNaN()
        val target = if (wasUnset) settledValue else targetValue
        closedAnchor = closed
        if (wasUnset || dragTarget == null && !userDragging) offsetState = positionOf(target)
    }

    private var userDragging by mutableStateOf(false)

    internal fun requireOffset(): Float {
        check(!offsetState.isNaN()) { "The offset was read before being initialized. Did you access the offset in a phase before layout, like effects or composition?" }
        return offsetState
    }

    private fun newOffsetForDelta(delta: Float): Float =
        ((if (offsetState.isNaN()) 0f else offsetState) + delta).coerceIn(minOf(closedAnchor, 0f), maxOf(closedAnchor, 0f))

    /** Runs [block] holding the drag mutex; afterwards the drawer settles on the anchor it rests on (if confirmed). */
    private suspend fun anchoredDrag(target: DrawerValue?, priority: MutatePriority, block: suspend () -> Unit) {
        if (target != null && !hasAnchors) {
            // No anchors yet: the value changes at once when confirmed (AnchoredDraggableState.anchoredDrag).
            if (confirmStateChange(target)) settledValue = target
            return
        }
        try {
            mutex.mutate(priority) {
                dragTarget = target
                try {
                    block()
                } finally {
                    dragTarget = null
                }
            }
        } finally {
            val closest = closestAnchor(offsetState)
            if (closest != null && abs(offsetState - positionOf(closest)) <= 0.5f && confirmStateChange(closest)) {
                settledValue = closest
            }
        }
    }

    private fun dragTo(newOffset: Float, velocity: Float) {
        offsetState = newOffset
        lastVelocity = velocity
    }

    /** User drag: deltas (pixels) move the sheet within the anchors. */
    internal val draggableState: DraggableState = object : DraggableState {
        private val scope = object : DragScope {
            override fun dragBy(pixels: Float) = dragTo(newOffsetForDelta(pixels), lastVelocity)
        }

        override suspend fun drag(dragPriority: MutatePriority, block: suspend DragScope.() -> Unit) {
            if (!hasAnchors) return
            anchoredDrag(null, dragPriority) {
                userDragging = true
                try {
                    scope.block()
                } finally {
                    userDragging = false
                }
            }
        }

        override fun dispatchRawDelta(delta: Float) {
            if (hasAnchors) offsetState = newOffsetForDelta(delta)
        }
    }

    /** Target after a release at [velocity] (AnchoredDraggableState.computeTarget). */
    private fun computeTarget(offset: Float, currentValue: DrawerValue, velocity: Float): DrawerValue {
        val currentAnchorPosition = positionOf(currentValue)
        val velocityThresholdPx = with(density ?: Guest.densityObject) { DrawerVelocityThreshold.toPx() }
        return if (currentAnchorPosition == offset || currentAnchorPosition.isNaN()) {
            currentValue
        } else if (abs(velocity) >= abs(velocityThresholdPx)) {
            closestAnchor(offset, sign(velocity) > 0)
        } else {
            val neighbor = closestAnchor(offset, offset - currentAnchorPosition > 0)
            val distance = abs(currentAnchorPosition - positionOf(neighbor))
            val relativeThreshold = abs(distance * DrawerPositionalThreshold)
            if (abs(currentAnchorPosition - offset) <= relativeThreshold) currentValue else neighbor
        }
    }

    /** Settles after a drag released at [velocity] (pixels per second), on the confirmed target. */
    internal suspend fun settle(velocity: Float) {
        if (!hasAnchors || offsetState.isNaN()) return
        val previous = settledValue
        val target = computeTarget(offsetState, previous, velocity)
        animateTo(if (confirmStateChange(target)) target else previous, anchoredDraggableMotionSpec, velocity)
    }

    // ---- public API -------------------------------------------------------------------------------------------------

    /** Spec of the settle after a drag. The source snapshots its initial value into the AnchoredDraggableState. */
    internal var anchoredDraggableMotionSpec: FiniteAnimationSpec<Float> = AnchoredDraggableDefaultAnimationSpec

    /** Whether the drawer is open. */
    val isOpen: Boolean
        get() = currentValue == DrawerValue.Open

    /** Whether the drawer is closed. */
    val isClosed: Boolean
        get() = currentValue == DrawerValue.Closed

    /**
     * The current value of the state: the value the drawer rests on, or the one it was on before a swipe or an
     * animation started.
     */
    val currentValue: DrawerValue
        get() = settledValue

    /** Whether the state is currently animating. */
    val isAnimationRunning: Boolean
        get() = dragTarget != null

    /** Open the drawer with animation and suspend until it is fully opened or the animation is cancelled. */
    suspend fun open(): Unit = animateTo(targetValue = DrawerValue.Open, animationSpec = openDrawerMotionSpec)

    /** Close the drawer with animation and suspend until it is fully closed or the animation is cancelled. */
    suspend fun close(): Unit = animateTo(targetValue = DrawerValue.Closed, animationSpec = closeDrawerMotionSpec)

    /** Set the state of the drawer with specific animation. */
    @Deprecated(
        message = "This method has been replaced by the open and close methods. The animation " +
            "spec is now an implementation detail of ModalDrawer.",
    )
    suspend fun animateTo(targetValue: DrawerValue, anim: AnimationSpec<Float>) {
        animateTo(targetValue = targetValue, animationSpec = anim)
    }

    /** Set the state without any animation and suspend until it's set. */
    suspend fun snapTo(targetValue: DrawerValue) {
        anchoredDrag(targetValue, MutatePriority.Default) { dragTo(positionOf(targetValue), 0f) }
    }

    /**
     * The target value of the drawer state: during a swipe the value the drawer would settle on (the closest anchor),
     * during an animation its target, otherwise [currentValue].
     */
    val targetValue: DrawerValue
        get() = dragTarget ?: offsetState.let { if (!it.isNaN()) closestAnchor(it) ?: settledValue else settledValue }

    /** The current position (in pixels) of the drawer sheet, or Float.NaN before the offset is initialized. */
    @Deprecated(
        message = "Please access the offset through currentOffset, which returns the value " +
            "directly instead of wrapping it in a state object.",
        replaceWith = ReplaceWith("currentOffset"),
    )
    val offset: State<Float> = object : State<Float> {
        override val value: Float get() = offsetState
    }

    /** The current position (in pixels) of the drawer sheet, or Float.NaN before the offset is initialized. */
    val currentOffset: Float
        get() = offsetState

    internal var density: Density? by mutableStateOf(null)

    internal var openDrawerMotionSpec: FiniteAnimationSpec<Float> = snap()

    internal var closeDrawerMotionSpec: FiniteAnimationSpec<Float> = snap()

    private suspend fun animateTo(targetValue: DrawerValue, animationSpec: AnimationSpec<Float>, velocity: Float = lastVelocity) {
        anchoredDrag(targetValue, MutatePriority.Default) {
            val targetOffset = positionOf(targetValue)
            if (!targetOffset.isNaN()) {
                val prev = if (offsetState.isNaN()) 0f else offsetState
                // An overshooting spring may go past the anchors: allowed, as in the source.
                animateFloat(prev, targetOffset, velocity, animationSpec) { value, v -> dragTo(value, v) }
            }
        }
    }

    companion object {
        /** The default [Saver] implementation for [DrawerState]. */
        fun Saver(confirmStateChange: (DrawerValue) -> Boolean): Saver<DrawerState, DrawerValue> =
            Saver(save = { it.currentValue }, restore = { DrawerState(it, confirmStateChange) })
    }
}

/** `animate(initialValue, targetValue, initialVelocity, animationSpec, block)` on the frame clock. */
private suspend fun animateFloat(from: Float, to: Float, velocity: Float, spec: AnimationSpec<Float>, block: (Float, Float) -> Unit) {
    val f = floatArrayOf(from)
    val t = floatArrayOf(to)
    val v = floatArrayOf(velocity)
    var start = -1L
    while (true) {
        val now = withFrameNanos { it }
        if (start < 0) start = now
        val (cur, vel, done) = sampleSpec(spec, f, t, v, now - start)
        block(cur[0], vel[0])
        if (done) break
    }
}

/**
 * Create and [remember] a [DrawerState].
 *
 * @param initialValue The initial value of the state.
 * @param confirmStateChange Optional callback invoked to confirm or veto a pending state change.
 */
@Composable
fun rememberDrawerState(
    initialValue: DrawerValue,
    confirmStateChange: (DrawerValue) -> Boolean = { true },
): DrawerState = rememberSaveable(saver = DrawerState.Saver(confirmStateChange)) {
    DrawerState(initialValue, confirmStateChange)
}

/** Where a drawer sheet reports the width it lays out at (the guest cannot measure), in dp. */
private class DrawerSheetWidth {
    var width by mutableStateOf<Dp?>(null)
}

private val LocalDrawerSheetWidth = staticCompositionLocalOf<DrawerSheetWidth?> { null }

/** The closed anchor of a drawer whose sheet is [sheet] wide: `-width` in pixels. */
private fun closedAnchorPx(sheet: Dp?): Float {
    val w = sheet ?: minOf(DrawerDefaults.MaximumDrawerWidth, Guest.screenWidth.dp)
    return -(w.value * Guest.density).roundToInt().toFloat()
}

// Standard motion scheme (MotionSchemeKeyTokens.DefaultSpatial / FastEffects, StandardMotionTokens).
private val DefaultSpatialSpec: FiniteAnimationSpec<Float> = spring(dampingRatio = 0.9f, stiffness = 700f)
private val FastEffectsSpec: FiniteAnimationSpec<Float> = spring(dampingRatio = 1f, stiffness = 3800f)

/**
 * [Material Design navigation drawer](https://m3.material.io/components/navigation-drawer/overview)
 *
 * Modal navigation drawers block interaction with the rest of an app's content with a scrim. They are elevated above
 * most of the app's UI and don't affect the screen's layout grid.
 *
 * @param drawerContent content inside this drawer
 * @param modifier the [Modifier] to be applied to this drawer
 * @param drawerState state of the drawer
 * @param gesturesEnabled whether or not the drawer can be interacted by gestures
 * @param scrimColor color of the scrim that obscures content when the drawer is open
 * @param content content of the rest of the UI
 */
@Composable
fun ModalNavigationDrawer(
    drawerContent: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    drawerState: DrawerState = rememberDrawerState(DrawerValue.Closed),
    gesturesEnabled: Boolean = true,
    scrimColor: Color = DrawerDefaults.scrimColor,
    content: @Composable () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val density = LocalDensity.current
    val sheetWidth = remember { DrawerSheetWidth() }
    val openMotion = DefaultSpatialSpec
    val closeMotion = FastEffectsSpec

    SideEffect {
        drawerState.density = density
        drawerState.openDrawerMotionSpec = openMotion
        drawerState.closeDrawerMotionSpec = closeMotion
    }
    // The anchors, from the reported sheet width (the source sets them when it places the measured sheet).
    val closed = closedAnchorPx(sheetWidth.width)
    SideEffect { drawerState.updateAnchors(closed) }

    Box(
        modifier
            .fillMaxSize()
            .draggable(
                state = drawerState.draggableState,
                orientation = Orientation.Horizontal,
                enabled = gesturesEnabled,
                startDragImmediately = drawerState.isAnimationRunning,
                onDragStopped = { velocity -> drawerState.settle(velocity) },
            ),
    ) {
        Box { content() }
        val offset = drawerState.currentOffset
        val fraction = if (offset.isNaN()) (if (drawerState.isOpen) 1f else 0f) else calculateFraction(closed, 0f, offset)
        val onDismissRequest = {
            if (gesturesEnabled && drawerState.confirmStateChange(DrawerValue.Closed)) {
                scope.launch { drawerState.close() }
            }
        }
        DrawerScrim(onClick = if (drawerState.isOpen) onDismissRequest else null, alpha = fraction, color = scrimColor)
        // The source composes the drawer content always but places it only while visible: a hidden sheet is moved
        // off screen and drawn at alpha 0.
        Box(
            Modifier.offset {
                val o = drawerState.currentOffset
                val x = when {
                    // The sheet stretches over an overshoot past the open anchor; here it holds at the edge.
                    !o.isNaN() -> minOf(o, 0f).roundToInt()
                    drawerState.isOpen -> 0
                    else -> -DrawerDefaults.MaximumDrawerWidth.roundToPx()
                }
                IntOffset(if (fraction == 0f) hiddenOffsetPx() else x, 0)
            }.then(if (fraction == 0f) Modifier.alpha(0f) else Modifier),
        ) {
            CompositionLocalProvider(LocalDrawerSheetWidth provides sheetWidth) { drawerContent() }
        }
    }
}

/** Where a composed but unplaced sheet goes: a screen width past the left edge. */
private fun hiddenOffsetPx() = -((Guest.screenWidth + MaximumDrawerWidthValue) * Guest.density).roundToInt()

/** The modal drawer's scrim: [color] at [alpha]; a tap calls [onClick] (when not null). */
@Composable
private fun DrawerScrim(onClick: (() -> Unit)?, alpha: Float, color: Color) {
    if (!color.isSpecified || alpha <= 0f) return
    val dismiss = if (onClick != null) {
        val current by rememberUpdatedState(onClick)
        Modifier.pointerInput(Unit) { detectTapGestures { current() } }
    } else {
        Modifier
    }
    Box(Modifier.fillMaxSize().then(dismiss).background(color.copy(alpha = color.alpha * alpha.coerceIn(0f, 1f))))
}

/**
 * [Material Design navigation drawer](https://m3.material.io/components/navigation-drawer/overview)
 *
 * Dismissible standard drawers sit next to app content: opening one moves the content to the side.
 *
 * @param drawerContent content inside this drawer
 * @param modifier the [Modifier] to be applied to this drawer
 * @param drawerState state of the drawer
 * @param gesturesEnabled whether or not the drawer can be interacted by gestures
 * @param content content of the rest of the UI
 */
@Composable
fun DismissibleNavigationDrawer(
    drawerContent: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    drawerState: DrawerState = rememberDrawerState(DrawerValue.Closed),
    gesturesEnabled: Boolean = true,
    content: @Composable () -> Unit,
) {
    val density = LocalDensity.current
    val sheetWidth = remember { DrawerSheetWidth() }
    val openMotion = DefaultSpatialSpec
    val closeMotion = FastEffectsSpec

    SideEffect {
        drawerState.density = density
        drawerState.openDrawerMotionSpec = openMotion
        drawerState.closeDrawerMotionSpec = closeMotion
    }
    val closed = closedAnchorPx(sheetWidth.width)
    SideEffect { drawerState.updateAnchors(closed) }

    Box(
        modifier.draggable(
            state = drawerState.draggableState,
            orientation = Orientation.Horizontal,
            enabled = gesturesEnabled,
            startDragImmediately = drawerState.isAnimationRunning,
            onDragStopped = { velocity -> drawerState.settle(velocity) },
        ),
        propagateMinConstraints = true,
    ) {
        val offset = drawerState.currentOffset
        val sheetOffset = if (offset.isNaN()) (if (drawerState.isOpen) 0f else closed) else offset
        // The content sits right of the sheet: sheet width + offset (the sheet is placed only while visible).
        val contentX = (-closed + sheetOffset).roundToInt()
        val visible = contentX != 0
        Box(
            Modifier.offset {
                val x = drawerState.currentOffset.let { if (it.isNaN()) sheetOffset else it }.roundToInt()
                IntOffset(if (visible) x else hiddenOffsetPx(), 0)
            }.then(if (visible) Modifier else Modifier.alpha(0f)),
        ) {
            CompositionLocalProvider(LocalDrawerSheetWidth provides sheetWidth) { drawerContent() }
        }
        Box(
            Modifier.offset {
                val o = drawerState.currentOffset.let { if (it.isNaN()) sheetOffset else it }
                IntOffset((-closed + o).roundToInt(), 0)
            },
        ) { content() }
    }
}

/**
 * [Material Design navigation permanent drawer](https://m3.material.io/components/navigation-drawer/overview)
 *
 * The permanent navigation drawer is always visible, next to the content.
 *
 * @param drawerContent content inside this drawer
 * @param modifier the [Modifier] to be applied to this drawer
 * @param content content of the rest of the UI
 */
@Composable
fun PermanentNavigationDrawer(
    drawerContent: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    Row(modifier.fillMaxSize()) {
        drawerContent()
        Box { content() }
    }
}

/**
 * Content inside of a modal navigation drawer. This version does not handle back; use the overload taking a
 * [DrawerState] for back handling and the predictive back animation.
 */
@Composable
fun ModalDrawerSheet(
    modifier: Modifier = Modifier,
    drawerShape: Shape = DrawerDefaults.shape,
    drawerContainerColor: Color = DrawerDefaults.modalContainerColor,
    drawerContentColor: Color = contentColorFor(drawerContainerColor),
    drawerTonalElevation: Dp = DrawerDefaults.ModalDrawerElevation,
    windowInsets: WindowInsets = DrawerDefaults.windowInsets,
    content: @Composable ColumnScope.() -> Unit,
) {
    DrawerSheet(null, windowInsets, modifier, drawerShape, drawerContainerColor, drawerContentColor, drawerTonalElevation, content)
}

/**
 * Content inside of a modal navigation drawer. Handles system back while the drawer is open (closing it), with the
 * predictive back animation.
 */
@Composable
fun ModalDrawerSheet(
    drawerState: DrawerState,
    modifier: Modifier = Modifier,
    drawerShape: Shape = DrawerDefaults.shape,
    drawerContainerColor: Color = DrawerDefaults.modalContainerColor,
    drawerContentColor: Color = contentColorFor(drawerContainerColor),
    drawerTonalElevation: Dp = DrawerDefaults.ModalDrawerElevation,
    windowInsets: WindowInsets = DrawerDefaults.windowInsets,
    content: @Composable ColumnScope.() -> Unit,
) {
    DrawerPredictiveBackHandler(drawerState) { backState ->
        DrawerSheet(backState, windowInsets, modifier, drawerShape, drawerContainerColor, drawerContentColor, drawerTonalElevation, content)
    }
}

/**
 * Content inside of a dismissible navigation drawer. This version does not handle back; use the overload taking a
 * [DrawerState] for back handling.
 */
@Composable
fun DismissibleDrawerSheet(
    modifier: Modifier = Modifier,
    drawerShape: Shape = RectangleShape,
    drawerContainerColor: Color = DrawerDefaults.standardContainerColor,
    drawerContentColor: Color = contentColorFor(drawerContainerColor),
    drawerTonalElevation: Dp = DrawerDefaults.DismissibleDrawerElevation,
    windowInsets: WindowInsets = DrawerDefaults.windowInsets,
    content: @Composable ColumnScope.() -> Unit,
) {
    DrawerSheet(null, windowInsets, modifier, drawerShape, drawerContainerColor, drawerContentColor, drawerTonalElevation, content)
}

/** Content inside of a dismissible navigation drawer. Handles system back while the drawer is open. */
@Composable
fun DismissibleDrawerSheet(
    drawerState: DrawerState,
    modifier: Modifier = Modifier,
    drawerShape: Shape = RectangleShape,
    drawerContainerColor: Color = DrawerDefaults.standardContainerColor,
    drawerContentColor: Color = contentColorFor(drawerContainerColor),
    drawerTonalElevation: Dp = DrawerDefaults.DismissibleDrawerElevation,
    windowInsets: WindowInsets = DrawerDefaults.windowInsets,
    content: @Composable ColumnScope.() -> Unit,
) {
    DrawerPredictiveBackHandler(drawerState) { backState ->
        DrawerSheet(backState, windowInsets, modifier, drawerShape, drawerContainerColor, drawerContentColor, drawerTonalElevation, content)
    }
}

/** Content inside of a permanent navigation drawer. */
@Composable
fun PermanentDrawerSheet(
    modifier: Modifier = Modifier,
    drawerShape: Shape = RectangleShape,
    drawerContainerColor: Color = DrawerDefaults.standardContainerColor,
    drawerContentColor: Color = contentColorFor(drawerContainerColor),
    drawerTonalElevation: Dp = DrawerDefaults.PermanentDrawerElevation,
    windowInsets: WindowInsets = DrawerDefaults.windowInsets,
    content: @Composable ColumnScope.() -> Unit,
) {
    DrawerSheet(null, windowInsets, modifier, drawerShape, drawerContainerColor, drawerContentColor, drawerTonalElevation, content)
}

@Composable
private fun DrawerSheet(
    backState: DrawerPredictiveBackState?,
    windowInsets: WindowInsets,
    modifier: Modifier,
    drawerShape: Shape,
    drawerContainerColor: Color,
    drawerContentColor: Color,
    drawerTonalElevation: Dp,
    content: @Composable ColumnScope.() -> Unit,
) {
    val maxWidth = NavigationDrawerTokens.ContainerWidth
    val reporter = LocalDrawerSheetWidth.current
    if (reporter != null) {
        val w = estimateSheetWidth(modifier)
        SideEffect { reporter.width = w }
    }
    // Predictive back: the container shrinks vertically (centred) by scaleYDistance instead of scaling.
    val shrink = with(LocalDensity.current) { (backState?.scaleYDistance ?: 0f).toDp() }
    Surface(
        modifier = modifier
            .sizeIn(minWidth = MinimumDrawerWidth, maxWidth = maxWidth)
            .then(if (shrink > 0.dp) Modifier.padding(vertical = shrink / 2) else Modifier)
            .fillMaxHeight(),
        shape = drawerShape,
        color = drawerContainerColor,
        contentColor = drawerContentColor,
        tonalElevation = drawerTonalElevation,
    ) {
        Column(
            Modifier.sizeIn(minWidth = MinimumDrawerWidth, maxWidth = maxWidth).windowInsetsPadding(windowInsets),
            content = content,
        )
    }
}

/**
 * The width a drawer sheet with [modifier] lays out at inside a drawer as wide as the screen, assuming its content
 * fills it (navigation items do): the outermost width modifier, then `sizeIn(240.dp, 360.dp)`.
 */
private fun estimateSheetWidth(modifier: Modifier): Dp {
    val screen = Guest.screenWidth
    var lo = 0f
    var hi = screen
    var fixed: Float? = null
    var done = false
    modifier.foldIn(Unit) { _, e ->
        if (done || e !is WireModifier) return@foldIn
        val j = e.json
        fun num(key: String) = Regex("\"$key\":(-?[0-9.]+)").find(j)?.groupValues?.get(1)?.toFloatOrNull()
        when {
            j.startsWith("{\"requiredWidth\":") -> { fixed = num("requiredWidth"); done = true }
            j.startsWith("{\"width\":") -> { fixed = num("width")?.coerceIn(lo, hi); done = true }
            j.startsWith("{\"fillMaxWidth\":") -> { fixed = (hi * (num("fillMaxWidth") ?: 1f)).coerceIn(lo, hi); done = true }
            j.startsWith("{\"widthIn\":") -> {
                val m = Regex("\\[(null|-?[0-9.]+),(null|-?[0-9.]+)]").find(j)
                m?.groupValues?.get(1)?.toFloatOrNull()?.let { lo = it.coerceIn(lo, hi) }
                m?.groupValues?.get(2)?.toFloatOrNull()?.let { hi = it.coerceIn(lo, hi) }
            }
            j.startsWith("{\"padding\":") -> {
                val p = Regex("\\[(-?[0-9.]+),(-?[0-9.]+),(-?[0-9.]+),(-?[0-9.]+)]").find(j)?.groupValues
                if (p != null) {
                    val h = p[1].toFloat() + p[3].toFloat()
                    hi = (hi - h).coerceAtLeast(0f)
                    lo = (lo - h).coerceAtLeast(0f)
                }
            }
        }
    }
    fixed?.let { return it.dp }
    // sizeIn(240, 360) coerced into [lo, hi]; filling content takes the maximum.
    return MaximumDrawerWidthValue.coerceIn(lo, hi).coerceAtLeast(MinimumDrawerWidth.value.coerceIn(lo, hi)).dp
}

/** Registers a [PredictiveBackHandler] (enabled while the drawer is open) that closes the drawer on back. */
@Composable
private fun DrawerPredictiveBackHandler(
    drawerState: DrawerState,
    content: @Composable (DrawerPredictiveBackState) -> Unit,
) {
    val backState = remember { DrawerPredictiveBackState() }
    val scope = rememberCoroutineScope()
    val density = LocalDensity.current
    val maxScaleXDistanceGrow = with(density) { PredictiveBackDrawerMaxScaleXDistanceGrow.toPx() }
    val maxScaleXDistanceShrink = with(density) { PredictiveBackDrawerMaxScaleXDistanceShrink.toPx() }
    val maxScaleYDistance = with(density) { PredictiveBackDrawerMaxScaleYDistance.toPx() }

    PredictiveBackHandler(enabled = drawerState.targetValue == DrawerValue.Open) { progress ->
        try {
            progress.collect { backEvent ->
                backState.update(
                    PredictiveBackEasing.transform(backEvent.progress),
                    backEvent.swipeEdge == BackEventCompat.EDGE_LEFT,
                    false,
                    maxScaleXDistanceGrow,
                    maxScaleXDistanceShrink,
                    maxScaleYDistance,
                )
            }
        } catch (e: CancellationException) {
            backState.clear()
        } finally {
            if (backState.swipeEdgeMatchesDrawer) {
                // Un-stretch smoothly while the drawer closes.
                scope.launch {
                    val from = backState.scaleXDistance
                    animateFloat(from, 0f, 0f, spring()) { value, _ -> backState.scaleXDistance = value }
                    backState.clear()
                }
            }
            drawerState.close()
        }
    }

    LaunchedEffect(drawerState.isClosed) {
        if (drawerState.isClosed) backState.clear()
    }

    content(backState)
}

/** Object to hold default values for [ModalNavigationDrawer] */
object DrawerDefaults {
    /** Default Elevation for drawer container in the [ModalNavigationDrawer]. */
    val ModalDrawerElevation: Dp = 0.dp

    /** Default Elevation for drawer container in the [PermanentNavigationDrawer]. */
    val PermanentDrawerElevation: Dp = NavigationDrawerTokens.StandardContainerElevation

    /** Default Elevation for drawer container in the [DismissibleNavigationDrawer]. */
    val DismissibleDrawerElevation: Dp = NavigationDrawerTokens.StandardContainerElevation

    /** Default shape for a navigation drawer (large end corners). */
    val shape: Shape
        @Composable get() = RoundedCornerShape(topStart = 0.dp, topEnd = 16.dp, bottomEnd = 16.dp, bottomStart = 0.dp)

    /** Default color of the scrim that obscures content when the drawer is open */
    val scrimColor: Color
        @Composable get() = MaterialTheme.colorScheme.scrim.copy(alpha = 0.32f)

    /** Default container color for a navigation drawer */
    @Deprecated(
        message = "Please use standardContainerColor or modalContainerColor instead.",
        replaceWith = ReplaceWith("standardContainerColor"),
        level = DeprecationLevel.WARNING,
    )
    val containerColor: Color
        @Composable get() = MaterialTheme.colorScheme.surface

    /** Default container color for a [DismissibleNavigationDrawer] and [PermanentNavigationDrawer] */
    val standardContainerColor: Color
        @Composable get() = MaterialTheme.colorScheme.surface

    /** Default container color for a [ModalNavigationDrawer] */
    val modalContainerColor: Color
        @Composable get() = MaterialTheme.colorScheme.surfaceContainerLow

    /** Default and maximum width of a navigation drawer */
    val MaximumDrawerWidth: Dp = NavigationDrawerTokens.ContainerWidth

    /** Default window insets for drawer sheets: the system bars' vertical and start sides. */
    val windowInsets: WindowInsets
        @Composable get() = WindowInsets.systemBars.only(WindowInsetsSides.Vertical + WindowInsetsSides.Start)
}

/**
 * Material Design navigation drawer item: a destination within a [ModalNavigationDrawer],
 * [PermanentNavigationDrawer] or [DismissibleNavigationDrawer].
 *
 * @param label text label for this item
 * @param selected whether this item is selected
 * @param onClick called when this item is clicked
 * @param modifier the [Modifier] to be applied to this item
 * @param icon optional icon for this item
 * @param badge optional badge to show on this item from the end side
 * @param shape optional shape for the active indicator
 * @param colors [NavigationDrawerItemColors] used to resolve the colors of this item
 * @param interactionSource an optional hoisted [MutableInteractionSource] for observing and emitting [Interaction]s
 */
@Composable
fun NavigationDrawerItem(
    label: @Composable () -> Unit,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    icon: (@Composable () -> Unit)? = null,
    badge: (@Composable () -> Unit)? = null,
    shape: Shape = CircleShape,
    colors: NavigationDrawerItemColors = NavigationDrawerItemDefaults.colors(),
    interactionSource: MutableInteractionSource? = null,
) {
    Surface(
        selected = selected,
        onClick = onClick,
        modifier = modifier
            .heightIn(min = NavigationDrawerTokens.ActiveIndicatorHeight)
            .fillMaxWidth(),
        shape = shape,
        color = colors.containerColor(selected).value,
        interactionSource = interactionSource,
    ) {
        Row(Modifier.padding(start = 16.dp, end = 24.dp), verticalAlignment = Alignment.CenterVertically) {
            if (icon != null) {
                val iconColor = colors.iconColor(selected).value
                CompositionLocalProvider(LocalContentColor provides iconColor, content = icon)
                Spacer(Modifier.width(12.dp))
            }
            Box(Modifier.weight(1f)) {
                val labelColor = colors.textColor(selected).value
                CompositionLocalProvider(LocalContentColor provides labelColor, content = label)
            }
            if (badge != null) {
                Spacer(Modifier.width(12.dp))
                val badgeColor = colors.badgeColor(selected).value
                CompositionLocalProvider(LocalContentColor provides badgeColor, content = badge)
            }
        }
    }
}

/** Represents the colors of the various elements of a drawer item. */
@Stable
interface NavigationDrawerItemColors {
    /** The icon color for this item, depending on whether it is [selected]. */
    @Composable fun iconColor(selected: Boolean): State<Color>

    /** The text color for this item, depending on whether it is [selected]. */
    @Composable fun textColor(selected: Boolean): State<Color>

    /** The badge color for this item, depending on whether it is [selected]. */
    @Composable fun badgeColor(selected: Boolean): State<Color>

    /** The container color for this item, depending on whether it is [selected]. */
    @Composable fun containerColor(selected: Boolean): State<Color>
}

/** Defaults used in [NavigationDrawerItem]. */
object NavigationDrawerItemDefaults {
    /** Creates a [NavigationDrawerItemColors] with the provided colors according to the Material specification. */
    @Composable
    fun colors(
        selectedContainerColor: Color = MaterialTheme.colorScheme.secondaryContainer,
        unselectedContainerColor: Color = Color.Transparent,
        selectedIconColor: Color = MaterialTheme.colorScheme.onSecondaryContainer,
        unselectedIconColor: Color = MaterialTheme.colorScheme.onSurfaceVariant,
        selectedTextColor: Color = MaterialTheme.colorScheme.onSecondaryContainer,
        unselectedTextColor: Color = MaterialTheme.colorScheme.onSurfaceVariant,
        selectedBadgeColor: Color = selectedTextColor,
        unselectedBadgeColor: Color = unselectedTextColor,
    ): NavigationDrawerItemColors = DefaultDrawerItemsColor(
        selectedIconColor, unselectedIconColor, selectedTextColor, unselectedTextColor,
        selectedContainerColor, unselectedContainerColor, selectedBadgeColor, unselectedBadgeColor,
    )

    /** Default external padding for a [NavigationDrawerItem] according to the Material specification. */
    val ItemPadding: PaddingValues = PaddingValues(horizontal = 12.dp)
}

@Stable
private class DrawerPredictiveBackState {
    var swipeEdgeMatchesDrawer by mutableStateOf(true)
    var scaleXDistance by mutableFloatStateOf(0f)
    var scaleYDistance by mutableFloatStateOf(0f)

    fun update(
        progress: Float,
        swipeEdgeLeft: Boolean,
        isRtl: Boolean,
        maxScaleXDistanceGrow: Float,
        maxScaleXDistanceShrink: Float,
        maxScaleYDistance: Float,
    ) {
        swipeEdgeMatchesDrawer = swipeEdgeLeft != isRtl
        val maxScaleXDistance = if (swipeEdgeMatchesDrawer) maxScaleXDistanceGrow else maxScaleXDistanceShrink
        scaleXDistance = maxScaleXDistance * progress
        scaleYDistance = maxScaleYDistance * progress
    }

    fun clear() {
        swipeEdgeMatchesDrawer = true
        scaleXDistance = 0f
        scaleYDistance = 0f
    }
}

private class DefaultDrawerItemsColor(
    val selectedIconColor: Color,
    val unselectedIconColor: Color,
    val selectedTextColor: Color,
    val unselectedTextColor: Color,
    val selectedContainerColor: Color,
    val unselectedContainerColor: Color,
    val selectedBadgeColor: Color,
    val unselectedBadgeColor: Color,
) : NavigationDrawerItemColors {
    @Composable
    override fun iconColor(selected: Boolean): State<Color> =
        rememberUpdatedState(if (selected) selectedIconColor else unselectedIconColor)

    @Composable
    override fun textColor(selected: Boolean): State<Color> =
        rememberUpdatedState(if (selected) selectedTextColor else unselectedTextColor)

    @Composable
    override fun containerColor(selected: Boolean): State<Color> =
        rememberUpdatedState(if (selected) selectedContainerColor else unselectedContainerColor)

    @Composable
    override fun badgeColor(selected: Boolean): State<Color> =
        rememberUpdatedState(if (selected) selectedBadgeColor else unselectedBadgeColor)

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is DefaultDrawerItemsColor) return false
        return selectedIconColor == other.selectedIconColor && unselectedIconColor == other.unselectedIconColor &&
            selectedTextColor == other.selectedTextColor && unselectedTextColor == other.unselectedTextColor &&
            selectedContainerColor == other.selectedContainerColor && unselectedContainerColor == other.unselectedContainerColor &&
            selectedBadgeColor == other.selectedBadgeColor && unselectedBadgeColor == other.unselectedBadgeColor
    }

    override fun hashCode(): Int {
        var result = selectedIconColor.hashCode()
        result = 31 * result + unselectedIconColor.hashCode()
        result = 31 * result + selectedTextColor.hashCode()
        result = 31 * result + unselectedTextColor.hashCode()
        result = 31 * result + selectedContainerColor.hashCode()
        result = 31 * result + unselectedContainerColor.hashCode()
        result = 31 * result + selectedBadgeColor.hashCode()
        result = 31 * result + unselectedBadgeColor.hashCode()
        return result
    }
}

/** NavigationDrawerTokens (material3 1.5.0-beta01) used here. */
private object NavigationDrawerTokens {
    val ActiveIndicatorHeight = 56.dp
    val ContainerWidth = 360.dp
    val StandardContainerElevation = 0.dp
}

private fun calculateFraction(a: Float, b: Float, pos: Float) = ((pos - a) / (b - a)).coerceIn(0f, 1f)

private const val DrawerPositionalThreshold = 0.5f
private val DrawerVelocityThreshold = 400.dp
private val MinimumDrawerWidth = 240.dp
private const val MaximumDrawerWidthValue = 360f

private val PredictiveBackDrawerMaxScaleXDistanceGrow = 12.dp
private val PredictiveBackDrawerMaxScaleXDistanceShrink = 24.dp
private val PredictiveBackDrawerMaxScaleYDistance = 48.dp

private val PredictiveBackEasing = CubicBezierEasing(0.1f, 0.1f, 0f, 1f)

// TODO: b/177571613 this should be a proper decay settling (from the source)
private val AnchoredDraggableDefaultAnimationSpec = TweenSpec<Float>(durationMillis = 256)
