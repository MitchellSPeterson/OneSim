package onesim.compose

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.AwaitPointerEventScope
import androidx.compose.ui.input.pointer.ConsumedData
import androidx.compose.ui.input.pointer.PointerEvent
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerEventTimeoutCancellationException
import androidx.compose.ui.input.pointer.PointerId
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.PointerInputEventHandler
import androidx.compose.ui.input.pointer.PointerInputScope
import androidx.compose.ui.input.pointer.PointerType
import androidx.compose.ui.platform.ViewConfiguration
import androidx.compose.ui.unit.IntSize
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlin.coroutines.Continuation
import kotlin.coroutines.CoroutineContext
import kotlin.coroutines.EmptyCoroutineContext
import kotlin.coroutines.createCoroutine
import kotlin.coroutines.intrinsics.COROUTINE_SUSPENDED
import kotlin.coroutines.intrinsics.suspendCoroutineUninterceptedOrReturn
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlin.math.roundToInt

/**
 * A node's pointer input handler (AndroidX's `SuspendingPointerInputModifierNode`): the [handler] coroutine starts at
 * the first event and reads events through [awaitPointerEventScope]. Events resume it synchronously, in pass order, so
 * consumption by one node is seen by the next.
 */
class PointerTarget(var handler: PointerInputEventHandler) : NodeState(), PointerInputScope {
    internal var keys: Array<out Any?> = emptyArray()
    override var size: IntSize = IntSize.Zero
        internal set
    override val density: Float get() = Guest.density
    override val viewConfiguration: ViewConfiguration get() = Guest.viewConfiguration
    override var interceptOutOfBoundsChildEvents: Boolean = false

    private var job: Job? = null
    /** Scopes waiting for an event. */
    private val waiting = ArrayList<EventScope<*>>()
    /** The last event this node received (all pointers up = no gesture in progress). */
    internal var lastEvent: PointerEvent? = null
    /** Uptime (ms) of the latest event, the base of `withTimeout` deadlines. */
    internal var now: Long = 0L

    /** Cancel the handler; it restarts at the next event. */
    fun reset() {
        val j = job
        job = null
        lastEvent = null
        j?.cancel(CancellationException("pointer input reset"))
        val pending = waiting.toList()
        waiting.clear()
        for (s in pending) s.fail(CancellationException("pointer input reset"))
    }

    override fun detach() = reset()

    private fun ensureStarted() {
        if (job?.isActive == true) return
        val h = handler
        job = Guest.scope.launch(start = CoroutineStart.UNDISPATCHED) { with(h) { invoke() } }
    }

    /** Fail the waiting scopes whose `withTimeout` ran out by event time [time] (before that event is dispatched). */
    internal fun expire(time: Long) {
        repeat(32) {
            val s = waiting.firstOrNull { it.deadline() <= time } ?: return
            val d = s.deadline()
            now = maxOf(now, d)
            s.fail(PointerEventTimeoutCancellationException(s.expired()?.millis ?: 0L))
        }
    }

    internal fun dispatch(event: PointerEvent, pass: PointerEventPass) {
        if (pass == PointerEventPass.Initial) {
            now = maxOf(now, event.changes.maxOfOrNull { it.uptimeMillis } ?: now)
            lastEvent = event
            ensureStarted()
        }
        val ready = waiting.filter { it.pass == pass }
        for (s in ready) if (s in waiting) s.deliver(event)
    }

    /** All pointers were taken away (AndroidX `onCancelPointerInput`): handlers see them lifted and consumed. */
    internal fun cancelPointerInput() {
        val last = lastEvent ?: return
        if (last.changes.none { it.pressed }) return
        val changes = last.changes.map { old ->
            PointerInputChange(
                old.id, old.uptimeMillis, old.position, false, old.pressure, old.uptimeMillis, old.position, old.pressed,
                old.type, ConsumedData().also { it.consumed = old.pressed },
            )
        }
        val cancel = PointerEvent(changes)
        lastEvent = cancel
        for (pass in PointerEventPass.entries) {
            val ready = waiting.filter { it.pass == pass }
            for (s in ready) if (s in waiting) s.deliver(cancel)
        }
        lastEvent = null
    }

    /**
     * As AndroidX: [block] runs as its own restricted coroutine (empty context, resumed directly by events); the caller
     * resumes through its dispatcher when it returns.
     */
    override suspend fun <R> awaitPointerEventScope(block: suspend AwaitPointerEventScope.() -> R): R =
        suspendCancellableCoroutine { outer ->
            val scope = EventScope(outer)
            outer.invokeOnCancellation { scope.fail(it ?: CancellationException("pointer input cancelled")) }
            block.createCoroutine(scope, scope).resume(Unit)
        }

    internal class Timeout(val millis: Long, val deadline: Long) {
        var active = true
    }

    /** One `awaitPointerEventScope` call, and the completion of its restricted coroutine. */
    inner class EventScope<R> internal constructor(private val outer: Continuation<R>) : AwaitPointerEventScope, Continuation<R> {
        override val context: CoroutineContext get() = EmptyCoroutineContext

        override fun resumeWith(result: Result<R>) {
            waiting.remove(this)
            cont = null
            outer.resumeWith(result)
        }

        internal var pass = PointerEventPass.Main
        private var cont: Continuation<PointerEvent>? = null
        private val timeouts = ArrayList<Timeout>()

        override val size: IntSize get() = this@PointerTarget.size
        override val density: Float get() = Guest.density
        override val viewConfiguration: ViewConfiguration get() = Guest.viewConfiguration
        override val currentEvent: PointerEvent get() = lastEvent ?: PointerEvent(emptyList())

        internal fun deadline(): Long = timeouts.filter { it.active }.minOfOrNull { it.deadline } ?: Long.MAX_VALUE
        internal fun expired(): Timeout? = timeouts.filter { it.active }.minByOrNull { it.deadline }

        override suspend fun awaitPointerEvent(pass: PointerEventPass): PointerEvent =
            suspendCoroutineUninterceptedOrReturn { c ->
                expired()?.takeIf { it.deadline <= now }?.let { throw PointerEventTimeoutCancellationException(it.millis) }
                this.pass = pass
                cont = c
                waiting.add(this)
                COROUTINE_SUSPENDED
            }

        internal fun deliver(event: PointerEvent) {
            val c = cont ?: return
            cont = null
            waiting.remove(this)
            c.resume(event)
        }

        internal fun fail(t: Throwable) {
            val c = cont ?: return
            cont = null
            waiting.remove(this)
            c.resumeWithException(t)
        }

        override suspend fun <T> withTimeout(timeMillis: Long, block: suspend AwaitPointerEventScope.() -> T): T {
            val t = Timeout(timeMillis, if (timeMillis <= 0L) now else now + timeMillis)
            timeouts.add(t)
            // Real time too: a finger resting with no events still times out.
            val timer = Guest.scope.launch {
                kotlinx.coroutines.delay(timeMillis.coerceAtLeast(0L))
                if (t.active && cont != null) fail(PointerEventTimeoutCancellationException(timeMillis))
            }
            try {
                return block()
            } finally {
                t.active = false
                timeouts.remove(t)
                timer.cancel()
            }
        }

        override suspend fun <T> withTimeoutOrNull(timeMillis: Long, block: suspend AwaitPointerEventScope.() -> T): T? =
            try {
                withTimeout(timeMillis, block)
            } catch (_: PointerEventTimeoutCancellationException) {
                null
            }
    }
}

/** Host touches (protocol section L `gesture`) turned into pointer events for the targets' handlers. */
internal object Pointers {
    private class Finger(val id: Long, var position: Offset, var time: Long, var pressed: Boolean)

    private var fingers = ArrayList<Finger>()
    private var nextId = 0L

    fun reset() {
        fingers = ArrayList()
    }

    /**
     * One touch message. [points] are screen points (dp); [targets] are (key, frame dp) innermost first.
     */
    fun dispatch(phase: String, time: Long, points: List<Offset>, targets: List<Pair<String, FloatArray>>) {
        val resolved = targets.mapNotNull { (key, frame) -> Guest.gestureTarget(key)?.let { it to frame } }
        if (phase == "cancel") {
            for ((t, _) in resolved) t.cancelPointerInput()
            fingers.clear()
            return
        }
        if (phase == "start") fingers.clear()
        val down = phase != "end"
        // (finger, previous position, previous time, previous pressed)
        val moves = ArrayList<Pair<Finger, Triple<Offset, Long, Boolean>>>()
        points.forEachIndexed { i, p ->
            val f = fingers.getOrNull(i)
            if (f == null || !f.pressed) {
                val n = Finger(nextId++, p, time, down)
                if (f == null) fingers.add(n) else fingers[i] = n
                moves.add(n to Triple(p, time, false))
            } else {
                val prev = Triple(f.position, f.time, f.pressed)
                f.position = p
                f.time = time
                f.pressed = down
                moves.add(f to prev)
            }
        }
        // Fingers the message no longer lists were lifted.
        for (f in fingers.drop(points.size)) {
            if (!f.pressed) continue
            moves.add(f to Triple(f.position, f.time, true))
            f.time = time
            f.pressed = false
        }
        val consumed = moves.map { ConsumedData() }
        val events = resolved.map { (t, frame) ->
            val d = Guest.density
            t.size = IntSize((frame[2] * d).roundToInt(), (frame[3] * d).roundToInt())
            val local = { p: Offset -> Offset((p.x - frame[0]) * d, (p.y - frame[1]) * d) }
            val changes = moves.mapIndexed { i, (f, prev) ->
                PointerInputChange(
                    PointerId(f.id), f.time, local(f.position), f.pressed, 1f, prev.second, local(prev.first), prev.third,
                    PointerType.Touch, consumed[i],
                )
            }
            t to PointerEvent(changes)
        }
        for ((t, e) in events) t.expire(time)
        val outerFirst = events.asReversed()
        for ((t, e) in outerFirst) t.dispatch(e, PointerEventPass.Initial)
        for ((t, e) in events) t.dispatch(e, PointerEventPass.Main)
        for ((t, e) in outerFirst) t.dispatch(e, PointerEventPass.Final)
        fingers.removeAll { !it.pressed }
    }
}
