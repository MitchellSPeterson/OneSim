package androidx.compose.foundation.gestures

import androidx.compose.foundation.MutatePriority
import androidx.compose.foundation.MutatorMutex
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.unit.Velocity
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import onesim.compose.Guest
import onesim.compose.NodeElement
import onesim.compose.NodeState
import onesim.compose.WireCx

/** Receives the drag deltas of a [DraggableState.drag] block. */
interface DragScope {
    fun dragBy(pixels: Float)
}

/** State of a one-axis [draggable]: deltas go to [dispatchRawDelta]; [drag] runs a drag (user or programmatic). */
interface DraggableState {
    suspend fun drag(dragPriority: MutatePriority = MutatePriority.Default, block: suspend DragScope.() -> Unit)

    fun dispatchRawDelta(delta: Float)
}

/** A [DraggableState] calling [onDelta] with each delta in pixels. */
fun DraggableState(onDelta: (Float) -> Unit): DraggableState = DefaultDraggableState(onDelta)

@Composable
fun rememberDraggableState(onDelta: (Float) -> Unit): DraggableState {
    val onDeltaState = rememberUpdatedState(onDelta)
    return remember { DraggableState { onDeltaState.value.invoke(it) } }
}

private class DefaultDraggableState(val onDelta: (Float) -> Unit) : DraggableState {
    private val dragScope = object : DragScope {
        override fun dragBy(pixels: Float) = onDelta(pixels)
    }
    private val scrollMutex = MutatorMutex()

    override suspend fun drag(dragPriority: MutatePriority, block: suspend DragScope.() -> Unit) = coroutineScope {
        scrollMutex.mutateWith(dragScope, dragPriority, block)
    }

    override fun dispatchRawDelta(delta: Float) = onDelta(delta)
}

private val NoOpOnDragStarted: suspend CoroutineScope.(startedPosition: Offset) -> Unit = {}
private val NoOpOnDragStopped: suspend CoroutineScope.(velocity: Float) -> Unit = {}

/**
 * Drag along [orientation]: past the touch slop (or at once with [startDragImmediately]) each move's delta along the axis
 * goes to [state]. [onDragStarted] gets the position where the drag began, [onDragStopped] the release velocity
 * (pixels per second along the axis). Emits [androidx.compose.foundation.interaction.DragInteraction]s.
 */
fun Modifier.draggable(
    state: DraggableState,
    orientation: Orientation,
    enabled: Boolean = true,
    interactionSource: MutableInteractionSource? = null,
    startDragImmediately: Boolean = false,
    onDragStarted: suspend CoroutineScope.(startedPosition: Offset) -> Unit = NoOpOnDragStarted,
    onDragStopped: suspend CoroutineScope.(velocity: Float) -> Unit = NoOpOnDragStopped,
    reverseDirection: Boolean = false,
): Modifier = this then DraggableElement(state, orientation, enabled, interactionSource, startDragImmediately, onDragStarted, onDragStopped, reverseDirection)

private class DraggableElement(
    val state: DraggableState,
    val orientation: Orientation,
    val enabled: Boolean,
    val interactionSource: MutableInteractionSource?,
    val startDragImmediately: Boolean,
    val onDragStarted: suspend CoroutineScope.(startedPosition: Offset) -> Unit,
    val onDragStopped: suspend CoroutineScope.(velocity: Float) -> Unit,
    val reverseDirection: Boolean,
) : NodeElement {
    override fun create(): NodeState = DraggableNodeState()

    override fun wire(state: NodeState, cx: WireCx) {
        val s = state as DraggableNodeState
        s.element = this
        s.orientationLock = orientation
        s.enabled = enabled
        s.interactionSource = interactionSource
        s.wireGesture(cx)
    }
}

private class DraggableNodeState : DragGestureState() {
    lateinit var element: DraggableElement

    private fun Float.reverseIfNeeded() = if (element.reverseDirection) -this else this

    override fun startDragImmediately() = element.startDragImmediately

    override suspend fun drag(forEachDelta: suspend ((DragEvent.DragDelta) -> Unit) -> Unit) {
        val e = element
        e.state.drag(MutatePriority.UserInput) {
            forEachDelta { dragBy(it.delta.along(e.orientation).reverseIfNeeded()) }
        }
    }

    override fun onDragStarted(startedPosition: Offset) {
        val cb = element.onDragStarted
        if (cb !== NoOpOnDragStarted) Guest.scope.launch { cb(startedPosition) }
    }

    override fun onDragStopped(velocity: Velocity) {
        val cb = element.onDragStopped
        val v = velocity.along(element.orientation).reverseIfNeeded()
        if (cb !== NoOpOnDragStopped) Guest.scope.launch { cb(v) }
    }
}
