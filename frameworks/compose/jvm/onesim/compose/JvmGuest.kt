package onesim.compose

import androidx.activity.ComponentActivity
import androidx.compose.runtime.BroadcastFrameClock
import androidx.compose.runtime.Composition
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Recomposer
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.yield
import java.io.PrintStream
import java.util.concurrent.CompletableFuture
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong
import kotlin.system.exitProcess

/**
 * JVM Compose guest entrypoint (protocol sections B and K): runs the app's composition on the real Compose runtime
 * with a [LayoutApplier], sends trees as JSON lines on the original stdout, and applies host messages from stdin.
 */
object JvmGuest {
    private val TYPE_RE = Regex(""""type"\s*:\s*"(\w+)"""")
    private val VERSION_RE = Regex(""""version"\s*:\s*(\d+)""")
    private val ID_RE = Regex(""""id"\s*:\s*(\d+)""")
    private val OFFSET_RE = Regex(""""offset"\s*:\s*(\d+)""")
    private val FIELD_RE = Regex(""""field"\s*:\s*(\d+)""")
    private val APPEARANCE_RE = Regex(""""appearance"\s*:\s*"(\w+)"""")
    private val TEXT_RE = Regex(""""text"\s*:\s*"((?:[^"\\]|\\.)*)"""")
    private val ESCAPE_RE = Regex("""\\(u[0-9a-fA-F]{4}|["\\/bfnrt])""")

    private lateinit var wire: PrintStream
    private val nextMeasureId = AtomicLong()
    private val measurements = ConcurrentHashMap<Long, CompletableFuture<String?>>()

    @Synchronized
    private fun send(line: String) {
        wire.print(line)
        wire.print('\n')
        wire.flush()
    }

    private fun error(message: String) = send("""{"type":"error","message":${androidx.compose.ui.jsonString(message)}}""")

    /** Serialize; with `force` a tree is sent even when unchanged (the host waits for one after each action). */
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
        Guest.uriSink = { uri ->
            val desktop = java.awt.Desktop.getDesktop()
            val target = java.net.URI(uri)
            if (target.scheme == "mailto") desktop.mail(target) else desktop.browse(target)
        }
        Guest.dark.value = System.getenv("SIMCORE_APPEARANCE") == "dark"
        Guest.density = System.getenv("SIMCORE_SCALE")?.toFloatOrNull() ?: 1f
        Guest.measureTextSink = ::measureText
        val parts = (System.getenv("SIMCORE_INSETS") ?: "0,0").split(',').map { it.trim().toFloatOrNull() ?: 0f }
        Guest.insetTop = parts.getOrElse(0) { 0f }
        Guest.insetBottom = parts.getOrElse(1) { 0f }

        val activityClass = args.firstOrNull() ?: run {
            System.err.println("usage: JvmGuest <ComponentActivity class>")
            exitProcess(2)
        }
        val messages = Channel<String>(Channel.UNLIMITED)
        Thread({
            val r = System.`in`.bufferedReader()
            while (true) {
                val line = r.readLine() ?: break
                if (measurementType.containsMatchIn(line)) {
                    val id = measurementId.find(line)?.groupValues?.get(1)?.toLongOrNull()
                    val result = measurementResult.find(line)?.groupValues?.get(1)
                    id?.let { measurements[it]?.complete(result) }
                } else {
                    messages.trySend(line)
                }
            }
            measurements.values.forEach { it.complete(null) }
            messages.close()
        }, "onesim-stdin").apply { isDaemon = true }.start()

        send("""{"type":"hello","protocol":1}""")
        runBlocking {
            val clock = BroadcastFrameClock()
            val ctx = coroutineContext + clock
            val recomposer = Recomposer(ctx)
            launch(ctx, start = CoroutineStart.UNDISPATCHED) { recomposer.runRecomposeAndApplyChanges() }

            /** Apply snapshot writes and run frames until the Recomposer is idle (bounded: endless animations). */
            suspend fun pump() {
                var frames = 0
                while (frames < 120) {
                    Snapshot.sendApplyNotifications()
                    repeat(4) { yield() }
                    if (clock.hasAwaiters) {
                        clock.sendFrame(System.nanoTime())
                        frames++
                        repeat(4) { yield() }
                    } else if (!recomposer.hasPendingWork) {
                        break
                    } else {
                        yield()
                    }
                }
            }

            // State written outside a host message (coroutines, effects): re-send once the runtime settles.
            var scheduled = false
            Snapshot.registerGlobalWriteObserver {
                if (!scheduled) {
                    scheduled = true
                    launch {
                        scheduled = false
                        pump()
                        emit(force = false)
                    }
                }
            }

            val app = try {
                val activity = Class.forName(activityClass).getDeclaredConstructor().newInstance() as ComponentActivity
                Guest.startActivity(activity)
            } catch (t: Throwable) {
                val cause = (t as? java.lang.reflect.InvocationTargetException)?.targetException ?: t
                error("activity $activityClass failed to start: ${cause.stackTraceToString()}")
                exitProcess(1)
            }
            val composition = Composition(LayoutApplier(Guest.root), recomposer)
            composition.setContent { CompositionLocalProvider(LocalDensity provides Density(Guest.density), content = app) }
            pump()
            emit(force = true)

            while (true) {
                val line = withContext(Dispatchers.IO) { messages.receiveCatching().getOrNull() } ?: break
                val type = TYPE_RE.find(line)?.groupValues?.get(1)
                when (type) {
                    "action", "text_input", "text_click", "text_layout" -> {
                        val v = VERSION_RE.find(line)?.groupValues?.get(1)?.toLongOrNull()
                        val before = Guest.version
                        try {
                            if (v == Guest.version) {
                                when (type) {
                                    "action" -> ID_RE.find(line)?.groupValues?.get(1)?.toLongOrNull()?.let { Guest.dispatchAction(it) }
                                    "text_input" -> textInput(line)?.let { text -> FIELD_RE.find(line)?.groupValues?.get(1)?.toLongOrNull()?.let { Guest.dispatchTextInput(it, text) } }
                                    "text_click" -> {
                                        val offset = OFFSET_RE.find(line)?.groupValues?.get(1)?.toLongOrNull()?.takeIf { it <= Int.MAX_VALUE }?.toInt()
                                        if (offset != null) ID_RE.find(line)?.groupValues?.get(1)?.toLongOrNull()?.let { Guest.dispatchTextClick(it, offset) }
                                    }
                                    "text_layout" -> Guest.dispatchTextLayouts(line)
                                }
                            }
                        } catch (t: Throwable) {
                            error("Input callback threw: ${t.stackTraceToString()}")
                        }
                        pump()
                        // The write observer may already have sent the new tree; otherwise send one (even unchanged).
                        emit(force = Guest.version == before)
                    }
                    "environment" -> {
                        APPEARANCE_RE.find(line)?.groupValues?.get(1)?.let { Guest.dark.value = it == "dark" }
                        pump()
                        emit(force = true)
                    }
                    "shutdown" -> break
                    else -> {}
                }
            }
            composition.dispose()
            recomposer.cancel()
        }
        exitProcess(0)
    }

    private val measurementType = Regex("""^\s*\{\s*"type"\s*:\s*"text_measurement"""")
    private val measurementId = Regex(""""id"\s*:\s*(\d+)""")
    private val measurementResult = Regex(""""result"\s*:\s*"([^"\\]*)"""")

    private fun measureText(request: String): String {
        val id = nextMeasureId.incrementAndGet()
        val response = CompletableFuture<String?>()
        measurements[id] = response
        try {
            send("""{"type":"measure_text","id":$id,"request":$request}""")
            return checkNotNull(response.get(10, TimeUnit.SECONDS)) { "OneSim could not measure text" }
        } finally {
            measurements.remove(id)
        }
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
