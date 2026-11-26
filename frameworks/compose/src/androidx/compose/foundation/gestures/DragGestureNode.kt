package androidx.compose.foundation.gestures

import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.PointerInputEventHandler
import androidx.compose.ui.input.pointer.changedToUpIgnoreConsumed
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.input.pointer.util.addPointerInputChange
import androidx.compose.ui.jsonString
import androidx.compose.ui.unit.Velocity
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import onesim.compose.Guest
import onesim.compose.NodeState
import onesim.compose.PointerTarget
import onesim.compose.WireCx

internal sealed class DragEvent {
    class DragStarted(val startPoint: Offset) : DragEvent()
    class DragStopped(val velocity: Velocity) : DragEvent()
    object DragCancelled : DragEvent()
    class DragDelta(val delta: Offset) : DragEvent()
}

/**
 * AndroidX's `DragGestureNode`, shared by `draggable`, `draggable2D` and `scrollable`: the pointer handler detects a drag
 * past the touch slop (along [orientationLock], or any direction) and sends events through a channel to a coroutine
 * that runs the drag inside the state's mutation ([drag]), emitting [DragInteraction]s.
 */
internal abstract class DragGestureState : NodeState() {
    var orientationLock: Orientation? = null
    var enabled = true
    var interactionSource: MutableInteractionSource? = null
    private var dragInteraction: DragInteraction.Start? = null
    private var channel: Channel<DragEvent>? = null
    private var listener: Job? = null
    private var handlerShape: Pair<Boolean, Orientation?>? = null
    val target = PointerTarget(PointerInputEventHandler { })

    abstract fun startDragImmediately(): Boolean
    abstract suspend fun drag(forEachDelta: suspend ((DragEvent.DragDelta) -> Unit) -> Unit)
    abstract fun onDragStarted(startedPosition: Offset)
    abstract fun onDragStopped(velocity: Velocity)

    /** Wire the gesture layer (when enabled). */
    fun wireGesture(cx: WireCx) {
        val shape = enabled to orientationLock
        if (shape != handlerShape) {
            handlerShape = shape
            target.reset()
            target.handler = handler()
        }
        if (!enabled) {
            disposeInteractionSource()
            return
        }
        startListening()
        val axis = when (orientationLock) {
            Orientation.Horizontal -> ""","axis":"horizontal""""
            Orientation.Vertical -> ""","axis":"vertical""""
            null -> ""
        }
        cx.add("""{"gesture":{"key":${jsonString(cx.gesture(target))},"pans":true$axis}}""")
    }

    private fun startListening() {
        if (listener?.isActive == true) return
        val ch = Channel<DragEvent>(capacity = Channel.UNLIMITED)
        channel = ch
        listener = Guest.scope.launch(start = CoroutineStart.UNDISPATCHED) {
            while (isActive) {
                var event = ch.receive()
                if (event !is DragEvent.DragStarted) continue
                processDragStart(event)
                try {
                    drag { processDelta ->
                        while (event !is DragEvent.DragStopped && event !is DragEvent.DragCancelled) {
                            (event as? DragEvent.DragDelta)?.let(processDelta)
                            event = ch.receive()
                        }
                    }
                    if (event is DragEvent.DragStopped) processDragStop(event as DragEvent.DragStopped)
                    else processDragCancel()
                } catch (c: CancellationException) {
                    processDragCancel()
                }
            }
        }
    }

    private suspend fun processDragStart(event: DragEvent.DragStarted) {
        dragInteraction?.let { interactionSource?.emit(DragInteraction.Cancel(it)) }
        val interaction = DragInteraction.Start()
        interactionSource?.emit(interaction)
        dragInteraction = interaction
        onDragStarted(event.startPoint)
    }

    private suspend fun processDragStop(event: DragEvent.DragStopped) {
        dragInteraction?.let { interactionSource?.emit(DragInteraction.Stop(it)) }
        dragInteraction = null
        onDragStopped(event.velocity)
    }

    private suspend fun processDragCancel() {
        dragInteraction?.let { interactionSource?.emit(DragInteraction.Cancel(it)) }
        dragInteraction = null
        onDragStopped(Velocity.Zero)
    }

    private fun disposeInteractionSource() {
        dragInteraction?.let { interactionSource?.tryEmit(DragInteraction.Cancel(it)) }
        dragInteraction = null
    }

    private fun handler() = PointerInputEventHandler {
        val velocityTracker = VelocityTracker()
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false)
            if (!enabled) return@awaitEachGesture
            velocityTracker.resetTracking()
            velocityTracker.addPointerInputChange(down)
            var drag: PointerInputChange?
            var overSlop = Offset.Zero
            if (startDragImmediately()) {
                down.consume()
                drag = down
            } else {
                do {
                    drag = awaitPointerSlopOrCancellation(down.id, down.type, orientationLock) { change, over ->
                        velocityTracker.addPointerInputChange(change)
                        change.consume()
                        overSlop = over
                    }
                } while (drag != null && !drag.isConsumed)
            }
            if (drag != null) {
                val ch = channel ?: return@awaitEachGesture
                ch.trySend(DragEvent.DragStarted(drag.position - overSlop))
                ch.trySend(DragEvent.DragDelta(overSlop))
                val onMove = { change: PointerInputChange ->
                    velocityTracker.addPointerInputChange(change)
                    val delta = change.positionChange()
                    change.consume()
                    ch.trySend(DragEvent.DragDelta(delta))
                    Unit
                }
                // A reset of this handler (disabled, new orientation, node removed) mid-drag must still end the
                // state's mutation, or a scrollable would stay in progress and block lower-priority scrolls.
                var ended = false
                val ok = try {
                    when (orientationLock) {
                        Orientation.Horizontal -> horizontalDrag(drag.id, onMove)
                        Orientation.Vertical -> verticalDrag(drag.id, onMove)
                        null -> drag(drag.id, onMove)
                    }.also { ended = true }
                } finally {
                    if (!ended) ch.trySend(DragEvent.DragCancelled)
                }
                if (ok) {
                    currentEvent.changes.forEach { if (it.changedToUpIgnoreConsumed()) velocityTracker.addPointerInputChange(it) }
                    val max = viewConfiguration.maximumFlingVelocity
                    val velocity = velocityTracker.calculateVelocity(Velocity(max, max))
                    velocityTracker.resetTracking()
                    ch.trySend(DragEvent.DragStopped(velocity))
                } else {
                    ch.trySend(DragEvent.DragCancelled)
                }
            }
        }
    }

    override fun detach() {
        target.reset()
        handlerShape = null
        listener?.cancel()
        listener = null
        channel = null
        disposeInteractionSource()
    }
}

internal fun Velocity.along(orientation: Orientation): Float = if (orientation == Orientation.Vertical) y else x
