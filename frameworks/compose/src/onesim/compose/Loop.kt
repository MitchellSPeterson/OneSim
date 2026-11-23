@file:OptIn(InternalCoroutinesApi::class)

package onesim.compose

import kotlinx.coroutines.CancellableContinuation
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Delay
import kotlinx.coroutines.DisposableHandle
import kotlinx.coroutines.InternalCoroutinesApi
import kotlinx.coroutines.Runnable
import kotlin.coroutines.CoroutineContext
import kotlin.coroutines.resume
import kotlin.time.TimeSource

/**
 * The guest's single-threaded event loop: a FIFO dispatcher with timers (`delay`, `withTimeout`) on the monotonic
 * wall clock. The platform guest drains it after each host message and wakes it when [nextDueIn] elapses.
 */
class GuestLoop : CoroutineDispatcher(), Delay {
    private class Timer(val due: Double, val seq: Long, val block: Runnable) : DisposableHandle {
        var cancelled = false
        override fun dispose() {
            cancelled = true
        }
    }

    private val start = TimeSource.Monotonic.markNow()
    private val queue = ArrayDeque<Runnable>()
    private val timers = ArrayList<Timer>()
    private var seq = 0L

    /** Milliseconds since the loop started. */
    fun nowMs(): Double = start.elapsedNow().inWholeNanoseconds / 1_000_000.0

    override fun dispatch(context: CoroutineContext, block: Runnable) {
        queue.addLast(block)
    }

    override fun scheduleResumeAfterDelay(timeMillis: Long, continuation: CancellableContinuation<Unit>) {
        val t = schedule(timeMillis, Runnable { continuation.resume(Unit) })
        continuation.invokeOnCancellation { t.dispose() }
    }

    override fun invokeOnTimeout(timeMillis: Long, block: Runnable, context: CoroutineContext): DisposableHandle =
        schedule(timeMillis, block)

    private fun schedule(timeMillis: Long, block: Runnable): Timer {
        val t = Timer(nowMs() + timeMillis.coerceIn(0L, Long.MAX_VALUE / 4).toDouble(), seq++, block)
        timers.add(t)
        return t
    }

    val hasQueued: Boolean get() = queue.isNotEmpty()

    /** Run queued tasks (bounded, so a coroutine that re-dispatches forever cannot hang the guest). */
    fun drain(limit: Int = 10_000) {
        var n = 0
        while (queue.isNotEmpty() && n < limit) {
            queue.removeFirst().run()
            n++
        }
    }

    /** Fire due timers in due order; returns whether any fired. */
    fun runDue(): Boolean {
        timers.removeAll { it.cancelled }
        val now = nowMs()
        val due = timers.filter { it.due <= now }.sortedWith(compareBy<Timer>({ it.due }, { it.seq }))
        if (due.isEmpty()) return false
        timers.removeAll(due.toSet())
        for (t in due) if (!t.cancelled) t.block.run()
        return true
    }

    /** Milliseconds until the next timer (0 when one is due), or null without timers. */
    fun nextDueIn(): Double? {
        timers.removeAll { it.cancelled }
        val next = timers.minOfOrNull { it.due } ?: return null
        return (next - nowMs()).coerceAtLeast(0.0)
    }

    fun clear() {
        queue.clear()
        timers.clear()
    }
}
