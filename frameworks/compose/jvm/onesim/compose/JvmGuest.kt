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
import java.io.PrintStream
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlin.system.exitProcess

/**
 * JVM Compose guest entrypoint (protocol sections B, K and L): runs the app's composition on the real Compose runtime
 * with a [LayoutApplier] on a single-threaded [GuestLoop], sends trees as JSON lines on the original stdout, and applies
 * host messages from stdin. Recomposition settles before each answer; animations that only wait for frames (ripples,
 * flings) run at 60 frames per second in real time, sending a tree per frame.
 */
object JvmGuest {
    private val TYPE_RE = Regex(""""type"\s*:\s*"(\w+)"""")
    private val VERSION_RE = Regex(""""version"\s*:\s*(\d+)""")
    private val ID_RE = Regex(""""id"\s*:\s*(\d+)""")
    private val FIELD_RE = Regex(""""field"\s*:\s*(\d+)""")
    private val APPEARANCE_RE = Regex(""""appearance"\s*:\s*"(\w+)"""")
    private val WIDTH_RE = Regex(""""width"\s*:\s*([0-9.]+)""")
    private val HEIGHT_RE = Regex(""""height"\s*:\s*([0-9.]+)""")
    private val TEXT_RE = Regex(""""text"\s*:\s*"((?:[^"\\]|\\.)*)"""")
    private val ESCAPE_RE = Regex("""\\(u[0-9a-fA-F]{4}|["\\/bfnrt])""")
    private const val FRAME_NS = 16_666_667L
    private val EOF = Any()

    private lateinit var wire: PrintStream

    private fun send(line: String) {
        wire.print(line)
        wire.print('\n')
        wire.flush()
    }

    private fun error(message: String) = send("""{"type":"error","message":${androidx.compose.ui.jsonString(message)}}""")

    /** Serialize; with `force` a tree is sent even when unchanged (the host waits for one after each input). */
    private fun emit(force: Boolean) {
        val tree = Guest.serialize(force) ?: return
        send("""{"type":"tree","version":${Guest.version},"root":$tree}""")
    }

    @JvmStatic
    fun main(args: Array<String>) {
        // The protocol owns the real stdout; app println and Logcat go to stderr (the host's session log).
        wire = PrintStream(java.io.FileOutputStream(java.io.FileDescriptor.out), false, "UTF-8")
        System.setOut(System.err)
        Guest.logSink = { System.err.println(it) }
        Guest.errorSink = { error(it) }
        Guest.messageSink = { send(it) }
        Guest.dark.value = System.getenv("SIMCORE_APPEARANCE") == "dark"
        System.getenv("SIMCORE_SCALE")?.toFloatOrNull()?.takeIf { it > 0f }?.let { Guest.density = it }
        val parts = (System.getenv("SIMCORE_INSETS") ?: "0,0").split(',').map { it.trim().toFloatOrNull() ?: 0f }
        Guest.insetTop = parts.getOrElse(0) { 0f }
        Guest.insetBottom = parts.getOrElse(1) { 0f }
        System.getenv("SIMCORE_VIEWPORT")?.split('x')?.mapNotNull { it.trim().toFloatOrNull() }?.takeIf { it.size == 2 }?.let {
            Guest.setViewport(it[0], it[1])
        }

        val activityClass = args.firstOrNull() ?: run {
            System.err.println("usage: JvmGuest <ComponentActivity class>")
            exitProcess(2)
        }
        val messages = LinkedBlockingQueue<Any>()
        Thread({
            val r = System.`in`.bufferedReader()
            while (true) {
                val line = r.readLine() ?: break
                messages.put(line)
            }
            messages.put(EOF)
        }, "onesim-stdin").apply { isDaemon = true }.start()

        send("""{"type":"hello","protocol":1}""")
        val loop = GuestLoop()
        val loopThread = Thread.currentThread()
        loop.onLoopThread = { Thread.currentThread() === loopThread }
        // Work resumed on another thread (e.g. after `withContext(Dispatchers.IO)`) wakes the loop like a host message.
        loop.offThread = { messages.put(it) }
        val clock = BroadcastFrameClock()
        val handler = CoroutineExceptionHandler { _, t -> error("Input callback threw: ${t.stackTraceToString()}") }
        val scope = CoroutineScope(loop + clock + SupervisorJob() + handler)
        Guest.scope = scope
        val recomposer = Recomposer(scope.coroutineContext)
        scope.launch(start = CoroutineStart.UNDISPATCHED) { recomposer.runRecomposeAndApplyChanges() }
        var lastFrame = 0L

        /** Apply snapshot writes and recompose until idle (bounded: endless composition-driven animations). */
        fun pump() {
            var frames = 0
            while (true) {
                // Queued work may write state: apply those writes before asking the Recomposer.
                loop.drain()
                Snapshot.sendApplyNotifications()
                loop.drain()
                // State the drained tasks wrote (an animation's last frame settling a value) must reach the
                // recomposer before deciding it is idle.
                Snapshot.sendApplyNotifications()
                if (!recomposer.hasPendingWork || frames >= 120) break
                if (clock.hasAwaiters) {
                    lastFrame = System.nanoTime()
                    clock.sendFrame(lastFrame)
                    frames++
                } else if (!loop.hasQueued) {
                    break
                }
            }
        }

        // State written outside a host message (timers, frame animations): re-send once settled.
        var dirty = false
        Snapshot.registerGlobalWriteObserver { dirty = true }

        val app = try {
            val activity = Class.forName(activityClass).getDeclaredConstructor().newInstance() as ComponentActivity
            Guest.startActivity(activity)
        } catch (t: Throwable) {
            val cause = (t as? java.lang.reflect.InvocationTargetException)?.targetException ?: t
            error("activity $activityClass failed to start: ${cause.stackTraceToString()}")
            exitProcess(1)
        }
        val composition = Composition(LayoutApplier(Guest.root), recomposer)
        composition.setContent(app)
        pump()
        emit(force = true)
        dirty = false

        while (true) {
            // Timers and frame-only animations (ripples, flings) advance in real time.
            var ran = loop.runDue()
            if (clock.hasAwaiters && System.nanoTime() - lastFrame >= FRAME_NS) {
                lastFrame = System.nanoTime()
                clock.sendFrame(lastFrame)
                ran = true
            }
            if (ran || dirty || loop.hasQueued) {
                dirty = false
                pump()
                emit(force = false)
            }
            var waitMs = loop.nextDueIn() ?: Double.MAX_VALUE
            if (clock.hasAwaiters) waitMs = minOf(waitMs, ((lastFrame + FRAME_NS - System.nanoTime()) / 1e6).coerceAtLeast(0.0))
            if (dirty || loop.hasQueued) waitMs = 0.0
            val next = if (waitMs >= Double.MAX_VALUE / 2) messages.take() else messages.poll((waitMs * 1000).toLong(), TimeUnit.MICROSECONDS)
            if (next === EOF) break
            if (next is Runnable) {
                loop.dispatch(kotlin.coroutines.EmptyCoroutineContext, next)
                continue
            }
            val line = next as? String ?: continue
            val type = TYPE_RE.find(line)?.groupValues?.get(1)
            when (type) {
                "action", "text_input", "gesture", "back" -> {
                    val before = Guest.version
                    try {
                        if (type == "gesture" || type == "back") {
                            Guest.dispatchGesture(line)
                        } else if (VERSION_RE.find(line)?.groupValues?.get(1)?.toLong() == Guest.version) {
                            if (type == "action") ID_RE.find(line)?.groupValues?.get(1)?.toLong()?.let { Guest.dispatchAction(it) }
                            else textInput(line)?.let { text -> FIELD_RE.find(line)?.groupValues?.get(1)?.toLong()?.let { Guest.dispatchTextInput(it, text) } }
                        }
                    } catch (t: Throwable) {
                        error("Input callback threw: ${t.stackTraceToString()}")
                    }
                    pump()
                    dirty = false
                    // Every input is answered with a tree, even an unchanged one.
                    emit(force = Guest.version == before)
                }
                "environment" -> {
                    APPEARANCE_RE.find(line)?.groupValues?.get(1)?.let { Guest.dark.value = it == "dark" }
                    val w = WIDTH_RE.find(line)?.groupValues?.get(1)?.toFloatOrNull() ?: 0f
                    val h = HEIGHT_RE.find(line)?.groupValues?.get(1)?.toFloatOrNull() ?: 0f
                    Guest.setViewport(w, h)
                    pump()
                    dirty = false
                    emit(force = true)
                }
                "shutdown" -> break
                else -> {}
            }
        }
        composition.dispose()
        recomposer.cancel()
        exitProcess(0)
    }

    private fun textInput(line: String): String? {
        val text = TEXT_RE.find(line)?.groupValues?.get(1) ?: return null
        return ESCAPE_RE.replace(text) { match ->
            when (val escape = match.groupValues[1]) {
                "b" -> "\b"
                "f" -> "\u000c"
                "n" -> "\n"
                "r" -> "\r"
                "t" -> "\t"
                else -> if (escape.startsWith('u')) escape.drop(1).toInt(16).toChar().toString() else escape
            }
        }
    }
}
