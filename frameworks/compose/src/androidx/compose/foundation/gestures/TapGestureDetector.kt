package androidx.compose.foundation.gestures

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.AwaitPointerEventScope
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerEventTimeoutCancellationException
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.PointerInputScope
import androidx.compose.ui.input.pointer.changedToDown
import androidx.compose.ui.input.pointer.changedToDownIgnoreConsumed
import androidx.compose.ui.input.pointer.changedToUp
import androidx.compose.ui.input.pointer.isOutOfBounds
import androidx.compose.ui.unit.Density
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex

/** Receiver of `onPress` in [detectTapGestures]: wait for the press to end. */
interface PressGestureScope : Density {
    /** Suspend until the press is released (true) or cancelled (false). */
    suspend fun tryAwaitRelease(): Boolean

    /** Suspend until the press is released; throws when it is cancelled. */
    suspend fun awaitRelease()
}

private val NoPressGesture: suspend PressGestureScope.(Offset) -> Unit = { }

/** Repeat [block] for each gesture: after it returns (or is cancelled) wait until every pointer is up. */
suspend fun PointerInputScope.awaitEachGesture(block: suspend AwaitPointerEventScope.() -> Unit) {
    val currentContext = currentCoroutineContext()
    awaitPointerEventScope {
        while (currentContext.isActive) {
            try {
                block()
                awaitAllPointersUp()
            } catch (e: CancellationException) {
                if (currentContext.isActive) awaitAllPointersUp() else throw e
            }
        }
    }
}

@Deprecated("Use awaitEachGesture instead. forEachGesture() can drop events between gestures.", ReplaceWith("awaitEachGesture(block)"))
suspend fun PointerInputScope.forEachGesture(block: suspend PointerInputScope.() -> Unit) {
    val currentContext = currentCoroutineContext()
    while (currentContext.isActive) {
        try {
            block()
            awaitAllPointersUp()
        } catch (e: CancellationException) {
            if (currentContext.isActive) awaitAllPointersUp() else throw e
        }
    }
}

internal fun AwaitPointerEventScope.allPointersUp(): Boolean = currentEvent.changes.none { it.pressed }

internal suspend fun PointerInputScope.awaitAllPointersUp() {
    awaitPointerEventScope { awaitAllPointersUp() }
}

internal suspend fun AwaitPointerEventScope.awaitAllPointersUp() {
    if (!allPointersUp()) {
        do {
            val events = awaitPointerEvent(PointerEventPass.Final)
        } while (events.changes.any { it.pressed })
    }
}

/** Wait for the first pointer to go down (unconsumed, unless [requireUnconsumed] is false). */
suspend fun AwaitPointerEventScope.awaitFirstDown(
    requireUnconsumed: Boolean = true,
    pass: PointerEventPass = PointerEventPass.Main,
): PointerInputChange {
    var event: androidx.compose.ui.input.pointer.PointerEvent
    do {
        event = awaitPointerEvent(pass)
    } while (!event.changes.all { if (requireUnconsumed) it.changedToDown() else it.changedToDownIgnoreConsumed() })
    return event.changes[0]
}

/**
 * Wait for every pointer to go up: returns the up change, or null when a change was consumed or the pointer left the
 * node's bounds (the press is cancelled).
 */
suspend fun AwaitPointerEventScope.waitForUpOrCancellation(pass: PointerEventPass = PointerEventPass.Main): PointerInputChange? {
    while (true) {
        val event = awaitPointerEvent(pass)
        if (event.changes.all { it.changedToUp() }) return event.changes[0]
        if (event.changes.any { it.isConsumed || it.isOutOfBounds(size, extendedTouchPadding) }) return null
        // Consumed in the final pass (a parent took the gesture)?
        val consumeCheck = awaitPointerEvent(PointerEventPass.Final)
        if (consumeCheck.changes.any { it.isConsumed }) return null
    }
}

/**
 * Taps, double taps, long presses and presses: [onPress] runs at each down, [onTap] after a tap (delayed by the double
 * tap timeout when [onDoubleTap] is set), [onLongPress] when the finger stays down past the long press timeout.
 */
suspend fun PointerInputScope.detectTapGestures(
    onDoubleTap: ((Offset) -> Unit)? = null,
    onLongPress: ((Offset) -> Unit)? = null,
    onPress: suspend PressGestureScope.(Offset) -> Unit = NoPressGesture,
    onTap: ((Offset) -> Unit)? = null,
) = coroutineScope {
    val pressScope = PressGestureScopeImpl(this@detectTapGestures)
    awaitEachGesture {
        val down = awaitFirstDown()
        down.consume()
        launch { pressScope.reset() }
        if (onPress !== NoPressGesture) launch { pressScope.onPress(down.position) }
        val longPressTimeout = onLongPress?.let { viewConfiguration.longPressTimeoutMillis } ?: (Long.MAX_VALUE / 2)
        var upOrCancel: PointerInputChange? = null
        try {
            upOrCancel = withTimeout(longPressTimeout) { waitForUpOrCancellation() }
            if (upOrCancel == null) {
                launch { pressScope.cancel() }
            } else {
                upOrCancel.consume()
                launch { pressScope.release() }
            }
        } catch (_: PointerEventTimeoutCancellationException) {
            onLongPress?.invoke(down.position)
            consumeUntilUp()
            launch { pressScope.release() }
        }
        if (upOrCancel != null) {
            if (onDoubleTap == null) {
                onTap?.invoke(upOrCancel.position)
            } else {
                val secondDown = awaitSecondDown(upOrCancel)
                if (secondDown == null) {
                    onTap?.invoke(upOrCancel.position)
                } else {
                    launch { pressScope.reset() }
                    if (onPress !== NoPressGesture) launch { pressScope.onPress(secondDown.position) }
                    try {
                        withTimeout(longPressTimeout) {
                            val secondUp = waitForUpOrCancellation()
                            if (secondUp != null) {
                                secondUp.consume()
                                launch { pressScope.release() }
                                onDoubleTap(secondUp.position)
                            } else {
                                launch { pressScope.cancel() }
                                onTap?.invoke(upOrCancel.position)
                            }
                        }
                    } catch (_: PointerEventTimeoutCancellationException) {
                        onTap?.invoke(upOrCancel.position)
                        onLongPress?.invoke(secondDown.position)
                        consumeUntilUp()
                        launch { pressScope.release() }
                    }
                }
            }
        }
    }
}

/** Presses and taps only (no double tap or long press). */
internal suspend fun PointerInputScope.detectTapAndPress(
    onPress: suspend PressGestureScope.(Offset) -> Unit = NoPressGesture,
    onTap: ((Offset) -> Unit)? = null,
) {
    val pressScope = PressGestureScopeImpl(this)
    coroutineScope {
        awaitEachGesture {
            launch { pressScope.reset() }
            val down = awaitFirstDown().also { it.consume() }
            if (onPress !== NoPressGesture) launch { pressScope.onPress(down.position) }
            val up = waitForUpOrCancellation()
            if (up == null) {
                launch { pressScope.cancel() }
            } else {
                up.consume()
                launch { pressScope.release() }
                onTap?.invoke(up.position)
            }
        }
    }
}

private suspend fun AwaitPointerEventScope.consumeUntilUp() {
    do {
        val event = awaitPointerEvent()
        event.changes.forEach { it.consume() }
    } while (event.changes.any { it.pressed })
}

private suspend fun AwaitPointerEventScope.awaitSecondDown(firstUp: PointerInputChange): PointerInputChange? =
    withTimeoutOrNull(viewConfiguration.doubleTapTimeoutMillis) {
        val minUptime = firstUp.uptimeMillis + viewConfiguration.doubleTapMinTimeMillis
        var change: PointerInputChange
        do {
            change = awaitFirstDown()
        } while (change.uptimeMillis < minUptime)
        change
    }

/** Wait for a long press of [pointerId]: the change at the timeout, or null when it lifts, moves away or is consumed. */
suspend fun AwaitPointerEventScope.awaitLongPressOrCancellation(pointerId: androidx.compose.ui.input.pointer.PointerId): PointerInputChange? {
    if (allPointersUp()) return null
    val initialDown = currentEvent.changes.firstOrNull { it.id == pointerId } ?: return null
    var longPress: PointerInputChange? = null
    var currentDown = initialDown
    val longPressTimeout = viewConfiguration.longPressTimeoutMillis
    return try {
        withTimeout(longPressTimeout) {
            var finished = false
            while (!finished) {
                val event = awaitPointerEvent(PointerEventPass.Main)
                if (event.changes.all { it.changedToUp() }) finished = true
                if (event.changes.any { it.isConsumed || it.isOutOfBounds(size, extendedTouchPadding) }) finished = true
                // Pointer slop is not checked: a long press may move within the bounds.
                val consumeCheck = awaitPointerEvent(PointerEventPass.Final)
                if (consumeCheck.changes.any { it.isConsumed }) finished = true
                if (!finished) {
                    val down = event.changes.firstOrNull { it.id == currentDown.id }
                    if (down != null && down.pressed) {
                        currentDown = down
                        longPress = down
                    } else {
                        val newPressed = event.changes.firstOrNull { it.pressed }
                        if (newPressed != null) {
                            currentDown = newPressed
                            longPress = newPressed
                        } else {
                            finished = true
                        }
                    }
                }
            }
        }
        null
    } catch (_: PointerEventTimeoutCancellationException) {
        longPress ?: initialDown
    }
}

internal class PressGestureScopeImpl(density: Density) : PressGestureScope, Density by density {
    private var isReleased = false
    private var isCanceled = false
    private val mutex = Mutex(locked = false)

    fun cancel() {
        isCanceled = true
        if (mutex.isLocked) mutex.unlock()
    }

    fun release() {
        isReleased = true
        if (mutex.isLocked) mutex.unlock()
    }

    suspend fun reset() {
        mutex.lock()
        isReleased = false
        isCanceled = false
    }

    override suspend fun awaitRelease() {
        if (!tryAwaitRelease()) throw GestureCancellationException("The press gesture was canceled.")
    }

    override suspend fun tryAwaitRelease(): Boolean {
        if (!isReleased && !isCanceled) {
            mutex.lock()
            mutex.unlock()
        }
        return isReleased
    }
}

/** Thrown by [PressGestureScope.awaitRelease] when the press is cancelled. */
class GestureCancellationException(message: String? = null) : CancellationException(message)
