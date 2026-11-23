package androidx.compose.foundation.gestures

import androidx.compose.foundation.MutatePriority
import androidx.compose.foundation.MutatorMutex
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.unit.Velocity
import kotlinx.coroutines.coroutineScope
import onesim.compose.NodeElement
import onesim.compose.NodeState
import onesim.compose.WireCx

/** Receives the drag deltas of a [Draggable2DState.drag] block. */
interface Drag2DScope {
    fun dragBy(pixels: Offset)
}

/** State of a [draggable2D]: deltas go to [dispatchRawDelta]; [drag] runs a drag (user or programmatic). */
interface Draggable2DState {
    suspend fun drag(dragPriority: MutatePriority = MutatePriority.Default, block: suspend Drag2DScope.() -> Unit)

    fun dispatchRawDelta(delta: Offset)
}

/** A [Draggable2DState] calling [onDelta] with each delta in pixels. */
fun Draggable2DState(onDelta: (Offset) -> Unit): Draggable2DState = DefaultDraggable2DState(onDelta)

@Composable
fun rememberDraggable2DState(onDelta: (Offset) -> Unit): Draggable2DState {
    val onDeltaState = rememberUpdatedState(onDelta)
    return remember { Draggable2DState { onDeltaState.value.invoke(it) } }
}

private class DefaultDraggable2DState(val onDelta: (Offset) -> Unit) : Draggable2DState {
    private val dragScope = object : Drag2DScope {
        override fun dragBy(pixels: Offset) = onDelta(pixels)
    }
    private val mutex = MutatorMutex()

    override suspend fun drag(dragPriority: MutatePriority, block: suspend Drag2DScope.() -> Unit) = coroutineScope {
        mutex.mutateWith(dragScope, dragPriority, block)
    }

    override fun dispatchRawDelta(delta: Offset) = onDelta(delta)
}

private val NoOpOnDragStart: (startedPosition: Offset) -> Unit = {}
private val NoOpOnDragStop: (velocity: Velocity) -> Unit = {}

/**
 * Drag in any direction: past the touch slop (or at once with [startDragImmediately]) each move's delta goes to [state].
 * [onDragStarted] gets the position where the drag began, [onDragStopped] the release velocity (pixels per second).
 */
fun Modifier.draggable2D(
    state: Draggable2DState,
    enabled: Boolean = true,
    interactionSource: MutableInteractionSource? = null,
    startDragImmediately: Boolean = false,
    onDragStarted: (startedPosition: Offset) -> Unit = NoOpOnDragStart,
    onDragStopped: (velocity: Velocity) -> Unit = NoOpOnDragStop,
    reverseDirection: Boolean = false,
): Modifier = this then Draggable2DElement(state, enabled, interactionSource, startDragImmediately, onDragStarted, onDragStopped, reverseDirection)

private class Draggable2DElement(
    val state: Draggable2DState,
    val enabled: Boolean,
    val interactionSource: MutableInteractionSource?,
    val startDragImmediately: Boolean,
    val onDragStarted: (startedPosition: Offset) -> Unit,
    val onDragStopped: (velocity: Velocity) -> Unit,
    val reverseDirection: Boolean,
) : NodeElement {
    override fun create(): NodeState = Draggable2DNodeState()

    override fun wire(state: NodeState, cx: WireCx) {
        val s = state as Draggable2DNodeState
        s.element = this
        s.orientationLock = null
        s.enabled = enabled
        s.interactionSource = interactionSource
        s.wireGesture(cx)
    }
}

private class Draggable2DNodeState : DragGestureState() {
    lateinit var element: Draggable2DElement

    override fun startDragImmediately() = element.startDragImmediately

    override suspend fun drag(forEachDelta: suspend ((DragEvent.DragDelta) -> Unit) -> Unit) {
        val e = element
        e.state.drag(MutatePriority.UserInput) {
            forEachDelta { dragBy(if (e.reverseDirection) -it.delta else it.delta) }
        }
    }

    override fun onDragStarted(startedPosition: Offset) = element.onDragStarted(startedPosition)

    override fun onDragStopped(velocity: Velocity) = element.onDragStopped(if (element.reverseDirection) -velocity else velocity)
}
