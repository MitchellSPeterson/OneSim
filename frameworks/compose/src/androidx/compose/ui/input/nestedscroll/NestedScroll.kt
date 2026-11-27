package androidx.compose.ui.input.nestedscroll

import androidx.compose.runtime.Stable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.unit.Velocity
import kotlinx.coroutines.CoroutineScope
import onesim.compose.Guest
import onesim.compose.NodeElement
import onesim.compose.NodeState
import onesim.compose.WireCx

/** What caused a nested scroll delta. */
@kotlin.jvm.JvmInline
value class NestedScrollSource internal constructor(private val value: Int) {
    override fun toString() = when (this) {
        UserInput -> "UserInput"
        SideEffect -> "SideEffect"
        Wheel -> "Wheel"
        Relocate -> "Relocate"
        else -> "Invalid"
    }

    companion object {
        /** A finger (or other pointer) dragging. */
        val UserInput = NestedScrollSource(1)
        /** Not user input: a fling or a programmatic scroll. */
        val SideEffect = NestedScrollSource(2)
        val Wheel = NestedScrollSource(3)
        val Relocate = NestedScrollSource(4)

        @Deprecated("Use UserInput", ReplaceWith("NestedScrollSource.UserInput"))
        val Drag = UserInput

        @Deprecated("Use SideEffect", ReplaceWith("NestedScrollSource.SideEffect"))
        val Fling = SideEffect
    }
}

/**
 * Takes part in nested scrolling: a scrolling child offers each delta to its ancestors' connections before using it
 * ([onPreScroll], outermost first) and what it left after ([onPostScroll], innermost first); flings likewise.
 */
interface NestedScrollConnection {
    fun onPreScroll(available: Offset, source: NestedScrollSource): Offset = Offset.Zero

    fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset = Offset.Zero

    suspend fun onPreFling(available: Velocity): Velocity = Velocity.Zero

    suspend fun onPostFling(consumed: Velocity, available: Velocity): Velocity = Velocity.Zero
}

/** A link in the chain of nested scroll connections from a node to the root. */
internal class NestedScrollNode(var connection: NestedScrollConnection?, var parent: NestedScrollNode?) {
    fun preScroll(available: Offset, source: NestedScrollSource): Offset {
        val parentConsumed = parent?.preScroll(available, source) ?: Offset.Zero
        val self = connection?.onPreScroll(available - parentConsumed, source) ?: Offset.Zero
        return parentConsumed + self
    }

    fun postScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
        val self = connection?.onPostScroll(consumed, available, source) ?: Offset.Zero
        val parentConsumed = parent?.postScroll(consumed + self, available - self, source) ?: Offset.Zero
        return self + parentConsumed
    }

    suspend fun preFling(available: Velocity): Velocity {
        val parentConsumed = parent?.preFling(available) ?: Velocity.Zero
        val self = connection?.onPreFling(available - parentConsumed) ?: Velocity.Zero
        return parentConsumed + self
    }

    suspend fun postFling(consumed: Velocity, available: Velocity): Velocity {
        val self = connection?.onPostFling(consumed, available) ?: Velocity.Zero
        val parentConsumed = parent?.postFling(consumed + self, available - self) ?: Velocity.Zero
        return self + parentConsumed
    }
}

/**
 * Sends a node's own scrolling into the nested scroll chain: deltas and flings go to the ancestors' connections
 * (the node's own connection is not called). Attach with `Modifier.nestedScroll(connection, dispatcher)`.
 */
class NestedScrollDispatcher {
    internal var parent: NestedScrollNode? = null

    /** Scope for flings started from the node; the guest's main scope. */
    val coroutineScope: CoroutineScope get() = Guest.scope

    fun dispatchPreScroll(available: Offset, source: NestedScrollSource): Offset = parent?.preScroll(available, source) ?: Offset.Zero

    fun dispatchPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset =
        parent?.postScroll(consumed, available, source) ?: Offset.Zero

    suspend fun dispatchPreFling(available: Velocity): Velocity = parent?.preFling(available) ?: Velocity.Zero

    suspend fun dispatchPostFling(consumed: Velocity, available: Velocity): Velocity =
        parent?.postFling(consumed, available) ?: Velocity.Zero
}

/**
 * Joins the nested scroll chain: [connection] sees the scrolling of everything this modifies (later modifiers and the
 * children), and [dispatcher] sends this node's own scrolling to the ancestors.
 */
@Stable
fun Modifier.nestedScroll(connection: NestedScrollConnection, dispatcher: NestedScrollDispatcher? = null): Modifier =
    this then NestedScrollElement(connection, dispatcher)

private class NestedScrollElement(val connection: NestedScrollConnection, val dispatcher: NestedScrollDispatcher?) : NodeElement {
    override fun create(): NodeState = NestedScrollState()

    override fun wire(state: NodeState, cx: WireCx) {
        val s = state as NestedScrollState
        val parent = cx.nestedScrollParent()
        s.node.connection = connection
        s.node.parent = parent
        dispatcher?.parent = parent
        cx.pushNestedScroll(s.node)
    }

    override fun equals(other: Any?) = other is NestedScrollElement && other.connection == connection && other.dispatcher == dispatcher
    override fun hashCode() = connection.hashCode() * 31 + dispatcher.hashCode()
}

private class NestedScrollState : NodeState() {
    val node = NestedScrollNode(null, null)

    override fun detach() {
        node.parent = null
        node.connection = null
    }
}
