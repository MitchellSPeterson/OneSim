package onesim.compose

import androidx.activity.ComponentActivity
import androidx.compose.runtime.BroadcastFrameClock
import androidx.compose.runtime.Composition
import androidx.compose.runtime.Recomposer
import androidx.compose.runtime.snapshots.Snapshot
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

/**
 * Browser (Kotlin/Wasm) Compose guest host: drives the real AndroidX Compose runtime synchronously on a [GuestLoop] and
 * returns serialized `ViewNode` JSON trees to the JS bridge (`web/onesim.js`), which submits them to `onesim-web`.
 * Frames are synthetic (16.7 ms apart) and run until idle within each input turn, so animations complete at once;
 * timers (`delay`, pointer timeouts) run on the wall clock through [nextTimer] / [onTimers].
 */
object WasmGuest {
    private var loop = GuestLoop()
    private val clock = BroadcastFrameClock()
    private var recomposer: Recomposer? = null
    private var composition: Composition? = null
    private var frameTimeNs = 1_000_000L

    private fun pump() {
        val rec = recomposer ?: return
        var frames = 0
        while (frames < 120) {
            // Queued work may write state: apply those writes before asking the Recomposer.
            loop.drain()
            Snapshot.sendApplyNotifications()
            loop.drain()
            if (clock.hasAwaiters) {
                frameTimeNs += 16_666_667L
                clock.sendFrame(frameTimeNs)
                frames++
                loop.drain()
            } else if (!rec.hasPendingWork && !loop.hasQueued) {
                break
            }
        }
    }

    fun start(dark: Boolean, insetTop: Float, insetBottom: Float, createActivity: () -> ComponentActivity): String {
        composition?.dispose()
        recomposer?.cancel()
        loop.clear()
        loop = GuestLoop()
        Guest.reset()
        Guest.dark.value = dark
        Guest.insetTop = insetTop
        Guest.insetBottom = insetBottom

        val handler = CoroutineExceptionHandler { _, t -> Guest.errorSink("Input callback threw: $t") }
        val scope = CoroutineScope(loop + clock + SupervisorJob() + handler)
        Guest.scope = scope
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

    /** One `gesture` message (protocol section L) as JSON. */
    fun onGesture(json: String): String {
        try {
            Guest.dispatchGesture(json)
        } catch (t: Throwable) {
            Guest.errorSink("Input callback threw: $t")
        }
        pump()
        return Guest.serialize(force = false) ?: ""
    }

    /** Milliseconds until the next timer is due, or -1 without timers. */
    fun nextTimer(): Double = loop.nextDueIn() ?: -1.0

    /** Run due timers; returns the new tree, or "" when unchanged. */
    fun onTimers(): String {
        loop.runDue()
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
