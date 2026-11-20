package onesim.compose

import androidx.activity.ComponentActivity
import androidx.compose.runtime.BroadcastFrameClock
import androidx.compose.runtime.Composition
import androidx.compose.runtime.Recomposer
import androidx.compose.runtime.snapshots.Snapshot
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Runnable
import kotlinx.coroutines.launch
import kotlin.coroutines.CoroutineContext

/** Synchronous FIFO dispatcher so Compose recomposition settles within each browser input turn. */
private class PumpDispatcher : CoroutineDispatcher() {
    private val queue = ArrayDeque<Runnable>()

    override fun dispatch(context: CoroutineContext, block: Runnable) {
        queue.addLast(block)
    }

    fun drain(limit: Int = 1000) {
        var count = 0
        while (queue.isNotEmpty() && count < limit) {
            queue.removeFirst().run()
            count++
        }
    }

    fun clear() {
        queue.clear()
    }
}

/**
 * Browser (Kotlin/Wasm) Compose guest host: drives the real AndroidX Compose runtime synchronously and returns
 * serialized `ViewNode` JSON trees to the JS bridge (`web/onesim.js`), which submits them to `onesim-web`.
 */
object WasmGuest {
    private val dispatcher = PumpDispatcher()
    private val clock = BroadcastFrameClock()
    private var recomposer: Recomposer? = null
    private var composition: Composition? = null
    private var frameTimeNs = 1_000_000L

    private fun pump() {
        val rec = recomposer ?: return
        var frames = 0
        while (frames < 120) {
            Snapshot.sendApplyNotifications()
            dispatcher.drain()
            if (clock.hasAwaiters) {
                frameTimeNs += 16_666_667L
                clock.sendFrame(frameTimeNs)
                frames++
                dispatcher.drain()
            } else if (!rec.hasPendingWork) {
                break
            } else {
                dispatcher.drain()
                if (!clock.hasAwaiters && !rec.hasPendingWork) break
            }
        }
    }

    fun start(dark: Boolean, insetTop: Float, insetBottom: Float, createActivity: () -> ComponentActivity): String {
        composition?.dispose()
        recomposer?.cancel()
        dispatcher.clear()
        Guest.reset()
        Guest.dark.value = dark
        Guest.insetTop = insetTop
        Guest.insetBottom = insetBottom

        val scope = CoroutineScope(dispatcher + clock)
        val rec = Recomposer(scope.coroutineContext)
        recomposer = rec
        scope.launch(start = CoroutineStart.UNDISPATCHED) { rec.runRecomposeAndApplyChanges() }

        val app = Guest.startActivity(createActivity())
        val comp = Composition(LayoutApplier(Guest.root), rec)
        composition = comp
        comp.setContent(app)
        pump()
        return Guest.serialize(force = true) ?: ""
    }

    fun onAction(id: Long): String {
        Guest.dispatchAction(id)
        pump()
        return Guest.serialize(force = false) ?: ""
    }

    fun onTextInput(field: Long, text: String): String {
        Guest.dispatchTextInput(field, text)
        pump()
        return Guest.serialize(force = false) ?: ""
    }

    fun onEnvironment(dark: Boolean, insetTop: Float, insetBottom: Float): String {
        Guest.dark.value = dark
        Guest.insetTop = insetTop
        Guest.insetBottom = insetBottom
        pump()
        return Guest.serialize(force = true) ?: ""
    }

    fun version(): Double = Guest.version.toDouble()
}
