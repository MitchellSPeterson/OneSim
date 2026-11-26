package androidx.compose.ui.input.pointer

import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.platform.ViewConfiguration
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.IntSize
import kotlinx.coroutines.CancellationException
import onesim.compose.NodeElement
import onesim.compose.NodeState
import onesim.compose.PointerTarget
import onesim.compose.WireCx
import kotlin.coroutines.RestrictsSuspension

/**
 * Receiver of a `pointerInput` block: the node's size and the device's touch configuration, and
 * [awaitPointerEventScope] to read events. The block starts at the first touch on the node and is restarted when its
 * keys change.
 */
interface PointerInputScope : Density {
    /** Size of the node in pixels, as last laid out. */
    val size: IntSize
    val extendedTouchPadding: Size get() = Size.Zero
    val viewConfiguration: ViewConfiguration
    var interceptOutOfBoundsChildEvents: Boolean
        get() = false
        set(@Suppress("UNUSED_PARAMETER") value) {}

    suspend fun <R> awaitPointerEventScope(block: suspend AwaitPointerEventScope.() -> R): R
}

/** Reads pointer events one by one; only its own members and extensions may suspend inside it. */
@RestrictsSuspension
interface AwaitPointerEventScope : Density {
    val size: IntSize
    val extendedTouchPadding: Size get() = Size.Zero
    /** The event being dispatched (or the last one). */
    val currentEvent: PointerEvent
    val viewConfiguration: ViewConfiguration

    /** Suspend until the next event reaches this node in [pass]. */
    suspend fun awaitPointerEvent(pass: PointerEventPass = PointerEventPass.Main): PointerEvent

    /** Run [block], returning null when no event came within [timeMillis]. */
    suspend fun <T> withTimeoutOrNull(timeMillis: Long, block: suspend AwaitPointerEventScope.() -> T): T? = block()

    /** Run [block]; throws [PointerEventTimeoutCancellationException] when no event came within [timeMillis]. */
    suspend fun <T> withTimeout(timeMillis: Long, block: suspend AwaitPointerEventScope.() -> T): T = block()
}

/** Thrown into an [AwaitPointerEventScope.withTimeout] block whose time ran out while it waited for an event. */
class PointerEventTimeoutCancellationException(time: Long) : CancellationException("Timed out waiting for $time ms")

/** The `pointerInput` block. */
fun interface PointerInputEventHandler {
    suspend operator fun PointerInputScope.invoke()
}

/** Handle touches on this node with [block], restarted when [key1] changes. */
fun Modifier.pointerInput(key1: Any?, block: PointerInputEventHandler): Modifier =
    this then SuspendPointerInputElement(arrayOf(key1), block)

/** Handle touches on this node with [block], restarted when [key1] or [key2] changes. */
fun Modifier.pointerInput(key1: Any?, key2: Any?, block: PointerInputEventHandler): Modifier =
    this then SuspendPointerInputElement(arrayOf(key1, key2), block)

/** Handle touches on this node with [block], restarted when any of [keys] changes. */
fun Modifier.pointerInput(vararg keys: Any?, block: PointerInputEventHandler): Modifier =
    this then SuspendPointerInputElement(arrayOf(*keys), block)

@Deprecated(
    "Modifier.pointerInput must provide one or more 'key' parameters that define the identity of the modifier and " +
        "determine when its previous input processing coroutine should be cancelled and a new effect launched for the new key.",
    level = DeprecationLevel.ERROR,
)
@Suppress("UNUSED_PARAMETER")
fun Modifier.pointerInput(block: PointerInputEventHandler): Modifier =
    error("Modifier.pointerInput must provide one or more 'key' parameters")

/** `pointerInput` on the wire: a `gesture` layer whose touches resume [block]. */
internal class SuspendPointerInputElement(
    private val keys: Array<out Any?>,
    private val handler: PointerInputEventHandler,
) : NodeElement {
    override fun create(): NodeState = PointerTarget(handler).also { it.keys = keys }

    override fun wire(state: NodeState, cx: WireCx) {
        val t = state as PointerTarget
        // Restart when a key changes or the block comes from another lambda (AndroidX compares the handler's class).
        if (!t.keys.contentEquals(keys) || t.handler::class != handler::class) {
            t.reset()
            t.keys = keys
        }
        t.handler = handler
        // The block may follow a moving finger, so a scroller under it does not take the touch.
        cx.add("""{"gesture":{"key":${androidx.compose.ui.jsonString(cx.gesture(t))},"pans":true}}""")
    }

    override fun equals(other: Any?) = other is SuspendPointerInputElement && keys.contentEquals(other.keys) && handler === other.handler
    override fun hashCode() = keys.contentHashCode() * 31 + handler.hashCode()
}
