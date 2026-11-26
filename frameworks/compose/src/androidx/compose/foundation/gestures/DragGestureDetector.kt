package androidx.compose.foundation.gestures

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.AwaitPointerEventScope
import androidx.compose.ui.input.pointer.PointerEvent
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerId
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.PointerInputScope
import androidx.compose.ui.input.pointer.PointerType
import androidx.compose.ui.input.pointer.changedToUp
import androidx.compose.ui.input.pointer.changedToUpIgnoreConsumed
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.input.pointer.positionChangeIgnoreConsumed
import androidx.compose.ui.input.pointer.positionChangedIgnoreConsumed
import androidx.compose.ui.platform.ViewConfiguration
import kotlinx.coroutines.CancellationException
import kotlin.math.abs
import kotlin.math.sign

/** The axis a gesture or scroller follows. */
enum class Orientation { Vertical, Horizontal }

internal fun Offset.along(orientation: Orientation): Float = if (orientation == Orientation.Vertical) y else x

internal fun ViewConfiguration.pointerSlop(pointerType: PointerType): Float =
    if (pointerType == PointerType.Mouse) touchSlop * 0.125f else touchSlop

/** Wait until [pointerId] moves past the touch slop in any direction; [onTouchSlopReached] gets the move past it. */
suspend fun AwaitPointerEventScope.awaitTouchSlopOrCancellation(
    pointerId: PointerId,
    onTouchSlopReached: (change: PointerInputChange, overSlop: Offset) -> Unit,
): PointerInputChange? = awaitPointerSlopOrCancellation(pointerId, PointerType.Touch, null) { c, o -> onTouchSlopReached(c, o) }

/** [awaitTouchSlopOrCancellation] measured along x only; the over-slop is the x distance past the slop. */
suspend fun AwaitPointerEventScope.awaitHorizontalTouchSlopOrCancellation(
    pointerId: PointerId,
    onTouchSlopReached: (change: PointerInputChange, overSlop: Float) -> Unit,
): PointerInputChange? = awaitPointerSlopOrCancellation(pointerId, PointerType.Touch, Orientation.Horizontal) { c, o -> onTouchSlopReached(c, o.x) }

/** [awaitTouchSlopOrCancellation] measured along y only; the over-slop is the y distance past the slop. */
suspend fun AwaitPointerEventScope.awaitVerticalTouchSlopOrCancellation(
    pointerId: PointerId,
    onTouchSlopReached: (change: PointerInputChange, overSlop: Float) -> Unit,
): PointerInputChange? = awaitPointerSlopOrCancellation(pointerId, PointerType.Touch, Orientation.Vertical) { c, o -> onTouchSlopReached(c, o.y) }

internal suspend fun AwaitPointerEventScope.awaitPointerSlopOrCancellation(
    pointerId: PointerId,
    pointerType: PointerType,
    orientation: Orientation?,
    onPointerSlopReached: (PointerInputChange, Offset) -> Unit,
): PointerInputChange? {
    if (currentEvent.isPointerUp(pointerId)) return null
    val touchSlop = viewConfiguration.pointerSlop(pointerType)
    var pointer = pointerId
    var total = Offset.Zero
    while (true) {
        val event = awaitPointerEvent()
        val dragEvent = event.changes.firstOrNull { it.id == pointer } ?: return null
        if (dragEvent.isConsumed) return null
        if (dragEvent.changedToUpIgnoreConsumed()) {
            val otherDown = event.changes.firstOrNull { it.pressed } ?: return null
            pointer = otherDown.id
        } else {
            total += dragEvent.position - dragEvent.previousPosition
            val inDirection = if (orientation == null) total.getDistance() else abs(total.along(orientation))
            if (inDirection < touchSlop) {
                awaitPointerEvent(PointerEventPass.Final)
                if (dragEvent.isConsumed) return null
            } else {
                val postSlop = if (orientation == null) {
                    total - total / inDirection * touchSlop
                } else {
                    val main = total.along(orientation)
                    val past = main - sign(main) * touchSlop
                    if (orientation == Orientation.Vertical) Offset(total.x, past) else Offset(past, total.y)
                }
                onPointerSlopReached(dragEvent, postSlop)
                if (dragEvent.isConsumed) return dragEvent
                total = Offset.Zero
            }
        }
    }
}

private fun PointerEvent.isPointerUp(pointerId: PointerId): Boolean =
    changes.firstOrNull { it.id == pointerId }?.pressed != true

/** Follow [pointerId] until it lifts (true) or a change is consumed (false), calling [onDrag] on each move. */
suspend fun AwaitPointerEventScope.drag(pointerId: PointerId, onDrag: (PointerInputChange) -> Unit): Boolean =
    dragLoop(pointerId, { it.positionChangedIgnoreConsumed() }, onDrag)

/** [drag] for moves along x. */
suspend fun AwaitPointerEventScope.horizontalDrag(pointerId: PointerId, onDrag: (PointerInputChange) -> Unit): Boolean =
    dragLoop(pointerId, { it.positionChangeIgnoreConsumed().x != 0f }, onDrag)

/** [drag] for moves along y. */
suspend fun AwaitPointerEventScope.verticalDrag(pointerId: PointerId, onDrag: (PointerInputChange) -> Unit): Boolean =
    dragLoop(pointerId, { it.positionChangeIgnoreConsumed().y != 0f }, onDrag)

private suspend fun AwaitPointerEventScope.dragLoop(
    pointerId: PointerId,
    hasDragged: (PointerInputChange) -> Boolean,
    onDrag: (PointerInputChange) -> Unit,
): Boolean {
    if (currentEvent.isPointerUp(pointerId)) return false
    var pointer = pointerId
    while (true) {
        val change = awaitDragOrUp(pointer, hasDragged) ?: return false
        if (change.isConsumed) return false
        if (change.changedToUpIgnoreConsumed()) return true
        onDrag(change)
        pointer = change.id
    }
}

private suspend fun AwaitPointerEventScope.awaitDragOrUp(
    pointerId: PointerId,
    hasDragged: (PointerInputChange) -> Boolean,
): PointerInputChange? {
    var pointer = pointerId
    while (true) {
        val event = awaitPointerEvent()
        val dragEvent = event.changes.firstOrNull { it.id == pointer } ?: return null
        if (dragEvent.changedToUpIgnoreConsumed()) {
            val otherDown = event.changes.firstOrNull { it.pressed } ?: return dragEvent
            pointer = otherDown.id
        } else if (hasDragged(dragEvent)) {
            return dragEvent
        }
    }
}

/** The next move of [pointerId] (unconsumed), its up, or null when it was consumed. */
suspend fun AwaitPointerEventScope.awaitDragOrCancellation(pointerId: PointerId): PointerInputChange? =
    awaitDragOrCancel(pointerId) { it.positionChangedIgnoreConsumed() }

suspend fun AwaitPointerEventScope.awaitHorizontalDragOrCancellation(pointerId: PointerId): PointerInputChange? =
    awaitDragOrCancel(pointerId) { it.positionChangeIgnoreConsumed().x != 0f }

suspend fun AwaitPointerEventScope.awaitVerticalDragOrCancellation(pointerId: PointerId): PointerInputChange? =
    awaitDragOrCancel(pointerId) { it.positionChangeIgnoreConsumed().y != 0f }

private suspend fun AwaitPointerEventScope.awaitDragOrCancel(
    pointerId: PointerId,
    hasDragged: (PointerInputChange) -> Boolean,
): PointerInputChange? {
    if (currentEvent.isPointerUp(pointerId)) return null
    val change = awaitDragOrUp(pointerId, hasDragged)
    return if (change?.isConsumed == false) change else null
}

/**
 * Drags in any direction: [onDragStart] once the touch slop is passed (with the pointer position), [onDrag] for each
 * move (the first one is the distance past the slop), then [onDragEnd] or [onDragCancel].
 */
suspend fun PointerInputScope.detectDragGestures(
    onDragStart: (Offset) -> Unit = { },
    onDragEnd: () -> Unit = { },
    onDragCancel: () -> Unit = { },
    onDrag: (change: PointerInputChange, dragAmount: Offset) -> Unit,
) = detectDragGestures(
    onDragStart = { _, slopTriggerChange, _ -> onDragStart(slopTriggerChange.position) },
    onDragEnd = { onDragEnd() },
    onDragCancel = onDragCancel,
    shouldAwaitTouchSlop = { true },
    orientationLock = null,
    onDrag = onDrag,
)

/**
 * Drags with full control: [shouldAwaitTouchSlop] false starts at the down, [orientationLock] measures the slop along
 * one axis only.
 */
suspend fun PointerInputScope.detectDragGestures(
    onDragStart: (down: PointerInputChange, slopTriggerChange: PointerInputChange, overSlop: Offset) -> Unit,
    onDragEnd: (change: PointerInputChange) -> Unit,
    onDragCancel: () -> Unit,
    shouldAwaitTouchSlop: () -> Boolean,
    orientationLock: Orientation?,
    onDrag: (change: PointerInputChange, dragAmount: Offset) -> Unit,
) {
    awaitEachGesture {
        val initialDown = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
        val awaitTouchSlop = shouldAwaitTouchSlop()
        if (!awaitTouchSlop) initialDown.consume()
        val down = awaitFirstDown(requireUnconsumed = false)
        var drag: PointerInputChange?
        var overSlop = Offset.Zero
        if (awaitTouchSlop) {
            do {
                drag = awaitPointerSlopOrCancellation(down.id, down.type, orientationLock) { change, over ->
                    change.consume()
                    overSlop = over
                }
            } while (drag != null && !drag.isConsumed)
        } else {
            drag = initialDown
        }
        if (drag != null) {
            onDragStart(down, drag, overSlop)
            onDrag(drag, overSlop)
            val upEvent = dragOrEnd(drag.id, orientationLock) { change ->
                onDrag(change, change.positionChange())
                change.consume()
            }
            if (upEvent == null) onDragCancel() else onDragEnd(upEvent)
        }
    }
}

/** [drag] along an optional axis; returns the up change, or null when cancelled. */
private suspend fun AwaitPointerEventScope.dragOrEnd(
    pointerId: PointerId,
    orientation: Orientation?,
    onDrag: (PointerInputChange) -> Unit,
): PointerInputChange? {
    if (currentEvent.isPointerUp(pointerId)) return null
    var pointer = pointerId
    val moved: (PointerInputChange) -> Boolean = when (orientation) {
        Orientation.Horizontal -> { c -> c.positionChangeIgnoreConsumed().x != 0f }
        Orientation.Vertical -> { c -> c.positionChangeIgnoreConsumed().y != 0f }
        null -> { c -> c.positionChangedIgnoreConsumed() }
    }
    while (true) {
        val change = awaitDragOrUp(pointer, moved) ?: return null
        if (change.isConsumed) return null
        if (change.changedToUpIgnoreConsumed()) return change
        onDrag(change)
        pointer = change.id
    }
}

/** Drags that start with a long press (no touch slop needed after it). */
suspend fun PointerInputScope.detectDragGesturesAfterLongPress(
    onDragStart: (Offset) -> Unit = { },
    onDragEnd: () -> Unit = { },
    onDragCancel: () -> Unit = { },
    onDrag: (change: PointerInputChange, dragAmount: Offset) -> Unit,
) {
    awaitEachGesture {
        try {
            val down = awaitFirstDown(requireUnconsumed = false)
            val drag = awaitLongPressOrCancellation(down.id)
            if (drag != null) {
                onDragStart.invoke(drag.position)
                if (drag(drag.id) { onDrag(it, it.positionChange()); it.consume() }) {
                    // Consume up if we quit drag gracefully with the up.
                    currentEvent.changes.forEach { if (it.changedToUp()) it.consume() }
                    onDragEnd()
                } else {
                    onDragCancel()
                }
            }
        } catch (c: CancellationException) {
            onDragCancel()
            throw c
        }
    }
}

/** Horizontal drags: the slop is measured along x and [onHorizontalDrag] gets x deltas. */
suspend fun PointerInputScope.detectHorizontalDragGestures(
    onDragStart: (Offset) -> Unit = { },
    onDragEnd: () -> Unit = { },
    onDragCancel: () -> Unit = { },
    onHorizontalDrag: (change: PointerInputChange, dragAmount: Float) -> Unit,
) = detectAxisDrag(Orientation.Horizontal, onDragStart, onDragEnd, onDragCancel, onHorizontalDrag)

/** Vertical drags: the slop is measured along y and [onVerticalDrag] gets y deltas. */
suspend fun PointerInputScope.detectVerticalDragGestures(
    onDragStart: (Offset) -> Unit = { },
    onDragEnd: () -> Unit = { },
    onDragCancel: () -> Unit = { },
    onVerticalDrag: (change: PointerInputChange, dragAmount: Float) -> Unit,
) = detectAxisDrag(Orientation.Vertical, onDragStart, onDragEnd, onDragCancel, onVerticalDrag)

private suspend fun PointerInputScope.detectAxisDrag(
    orientation: Orientation,
    onDragStart: (Offset) -> Unit,
    onDragEnd: () -> Unit,
    onDragCancel: () -> Unit,
    onDrag: (PointerInputChange, Float) -> Unit,
) {
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false)
        var overSlop = 0f
        val drag = awaitPointerSlopOrCancellation(down.id, down.type, orientation) { change, over ->
            change.consume()
            overSlop = over.along(orientation)
        }
        if (drag != null) {
            onDragStart.invoke(drag.position)
            onDrag(drag, overSlop)
            val done = if (orientation == Orientation.Horizontal) {
                horizontalDrag(drag.id) { onDrag(it, it.positionChange().x); it.consume() }
            } else {
                verticalDrag(drag.id) { onDrag(it, it.positionChange().y); it.consume() }
            }
            if (done) onDragEnd() else onDragCancel()
        }
    }
}
