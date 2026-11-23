package androidx.compose.ui.input.pointer

import androidx.compose.runtime.Immutable
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.unit.IntSize
import kotlin.jvm.JvmInline

/** Identifies one pointer (finger) for as long as it is down. */
@JvmInline
value class PointerId(val value: Long)

/** Kind of input device behind a pointer. OneSim only produces touches (a Pixel phone). */
@JvmInline
value class PointerType private constructor(private val value: Int) {
    override fun toString() = when (this) {
        Touch -> "Touch"; Mouse -> "Mouse"; Stylus -> "Stylus"; Eraser -> "Eraser"; else -> "Unknown"
    }

    companion object {
        val Unknown = PointerType(0)
        val Touch = PointerType(1)
        val Mouse = PointerType(2)
        val Stylus = PointerType(3)
        val Eraser = PointerType(4)
    }
}

/** What changed in a [PointerEvent]. */
@JvmInline
value class PointerEventType private constructor(internal val value: Int) {
    override fun toString() = when (this) {
        Press -> "Press"; Release -> "Release"; Move -> "Move"; Enter -> "Enter"; Exit -> "Exit"; Scroll -> "Scroll"; else -> "Unknown"
    }

    companion object {
        val Unknown = PointerEventType(0)
        val Press = PointerEventType(1)
        val Release = PointerEventType(2)
        val Move = PointerEventType(3)
        val Enter = PointerEventType(4)
        val Exit = PointerEventType(5)
        val Scroll = PointerEventType(6)
    }
}

/**
 * The three passes of pointer event dispatch along the hit path: [Initial] (ancestors before descendants), [Main]
 * (descendants before ancestors) and [Final] (ancestors before descendants again).
 */
enum class PointerEventPass { Initial, Main, Final }

/** Mouse buttons held during an event; always none for touches. */
@JvmInline
value class PointerButtons(internal val packedValue: Int)

val PointerButtons.isPrimaryPressed: Boolean get() = packedValue and 1 != 0
val PointerButtons.isSecondaryPressed: Boolean get() = packedValue and 2 != 0
val PointerButtons.isTertiaryPressed: Boolean get() = packedValue and 4 != 0
val PointerButtons.areAnyPressed: Boolean get() = packedValue != 0

/** Keyboard modifiers held during an event; always none on the simulated phone. */
@JvmInline
value class PointerKeyboardModifiers(internal val packedValue: Int)

val PointerKeyboardModifiers.isCtrlPressed: Boolean get() = packedValue and 1 != 0
val PointerKeyboardModifiers.isShiftPressed: Boolean get() = packedValue and 2 != 0
val PointerKeyboardModifiers.isAltPressed: Boolean get() = packedValue and 4 != 0
val PointerKeyboardModifiers.isMetaPressed: Boolean get() = packedValue and 8 != 0

/** A position of the pointer between two events (none: OneSim reports every touch sample as an event). */
@Immutable
class HistoricalChange(val uptimeMillis: Long, val position: Offset)

/** Consumption shared by every node's copy of one pointer's change in one event. */
internal class ConsumedData {
    var consumed = false
}

/**
 * One pointer's change in a [PointerEvent], in the receiving node's local pixel coordinates. Every node on the hit
 * path sees the same consumption: a change consumed by a child is consumed for its parents.
 */
@Immutable
class PointerInputChange internal constructor(
    val id: PointerId,
    val uptimeMillis: Long,
    val position: Offset,
    val pressed: Boolean,
    val pressure: Float,
    val previousUptimeMillis: Long,
    val previousPosition: Offset,
    val previousPressed: Boolean,
    val type: PointerType,
    internal val consumedData: ConsumedData,
    val scrollDelta: Offset = Offset.Zero,
    val historical: List<HistoricalChange> = emptyList(),
) {
    constructor(
        id: PointerId,
        uptimeMillis: Long,
        position: Offset,
        pressed: Boolean,
        previousUptimeMillis: Long,
        previousPosition: Offset,
        previousPressed: Boolean,
        isInitiallyConsumed: Boolean,
        type: PointerType = PointerType.Touch,
        scrollDelta: Offset = Offset.Zero,
    ) : this(id, uptimeMillis, position, pressed, 1f, previousUptimeMillis, previousPosition, previousPressed, type,
        ConsumedData().also { it.consumed = isInitiallyConsumed }, scrollDelta)

    /** Whether some node consumed this change (it then is not a tap, drag start, ...). */
    val isConsumed: Boolean get() = consumedData.consumed

    fun consume() {
        consumedData.consumed = true
    }

    fun copy(
        id: PointerId = this.id,
        currentTime: Long = this.uptimeMillis,
        currentPosition: Offset = this.position,
        currentPressed: Boolean = this.pressed,
        previousTime: Long = this.previousUptimeMillis,
        previousPosition: Offset = this.previousPosition,
        previousPressed: Boolean = this.previousPressed,
        type: PointerType = this.type,
        scrollDelta: Offset = this.scrollDelta,
    ) = PointerInputChange(id, currentTime, currentPosition, currentPressed, pressure, previousTime, previousPosition,
        previousPressed, type, consumedData, scrollDelta, historical)

    override fun toString() = "PointerInputChange(id=$id, uptimeMillis=$uptimeMillis, position=$position, pressed=$pressed, " +
        "previousUptimeMillis=$previousUptimeMillis, previousPosition=$previousPosition, previousPressed=$previousPressed, " +
        "isConsumed=$isConsumed, type=$type)"
}

/** All pointers' changes at one moment, as one node sees them. */
class PointerEvent(val changes: List<PointerInputChange>) {
    val buttons: PointerButtons get() = PointerButtons(0)
    val keyboardModifiers: PointerKeyboardModifiers get() = PointerKeyboardModifiers(0)
    var type: PointerEventType = calculateType()
        internal set

    private fun calculateType(): PointerEventType {
        for (c in changes) {
            if (c.changedToUpIgnoreConsumed()) return PointerEventType.Release
            if (c.changedToDownIgnoreConsumed()) return PointerEventType.Press
        }
        return PointerEventType.Move
    }

    override fun toString() = "PointerEvent(type=$type, changes=$changes)"
}

fun PointerInputChange.changedToDown() = !isConsumed && !previousPressed && pressed
fun PointerInputChange.changedToDownIgnoreConsumed() = !previousPressed && pressed
fun PointerInputChange.changedToUp() = !isConsumed && previousPressed && !pressed
fun PointerInputChange.changedToUpIgnoreConsumed() = previousPressed && !pressed
fun PointerInputChange.positionChanged() = positionChangeInternal(false) != Offset.Zero
fun PointerInputChange.positionChangedIgnoreConsumed() = positionChangeInternal(true) != Offset.Zero
fun PointerInputChange.positionChange() = positionChangeInternal(false)
fun PointerInputChange.positionChangeIgnoreConsumed() = positionChangeInternal(true)

private fun PointerInputChange.positionChangeInternal(ignoreConsumed: Boolean): Offset {
    val offset = position - previousPosition
    return if (!ignoreConsumed && isConsumed) Offset.Zero else offset
}

@Deprecated("Use isConsumed instead", ReplaceWith("isConsumed"))
fun PointerInputChange.anyChangeConsumed() = isConsumed

@Deprecated("Use consume() instead", ReplaceWith("if (pressed != previousPressed) consume()"))
fun PointerInputChange.consumeDownChange() {
    if (pressed != previousPressed) consume()
}

@Deprecated("Use consume() instead", ReplaceWith("if (positionChange() != Offset.Zero) consume()"))
fun PointerInputChange.consumePositionChange() {
    if (positionChange() != Offset.Zero) consume()
}

@Deprecated("Use consume() instead", ReplaceWith("consume()"))
fun PointerInputChange.consumeAllChanges() = consume()

/** Whether the pointer is outside a node of [size], allowing [extendedTouchPadding] around it. */
fun PointerInputChange.isOutOfBounds(size: IntSize, extendedTouchPadding: Size = Size.Zero): Boolean {
    val (x, y) = position
    return x < -extendedTouchPadding.width || x > size.width + extendedTouchPadding.width ||
        y < -extendedTouchPadding.height || y > size.height + extendedTouchPadding.height
}

@Deprecated("Use isOutOfBounds() that supports minimum touch target", ReplaceWith("this.isOutOfBounds(size, Size.Zero)"))
fun PointerInputChange.isOutOfBounds(size: IntSize): Boolean = isOutOfBounds(size, Size.Zero)
