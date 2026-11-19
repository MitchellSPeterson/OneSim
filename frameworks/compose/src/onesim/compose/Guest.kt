package onesim.compose

import androidx.activity.ComponentActivity
import androidx.compose.runtime.BroadcastFrameClock
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Composition
import androidx.compose.runtime.Recomposer
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.snapshots.Snapshot
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.yield
import java.io.PrintStream
import kotlin.system.exitProcess

/**
 * The Compose guest (protocol sections B and K): runs the app's composition on the real Compose runtime with a
 * [LayoutApplier], sends the tree as JSON lines on the original stdout and applies host messages from stdin.
 */
object Guest {
    /** `isSystemInDarkTheme()`; the host's `environment` message updates it. */
    val dark = mutableStateOf(System.getenv("SIMCORE_APPEARANCE") == "dark")
    val insetTop: Float
    val insetBottom: Float

    init {
        val parts = (System.getenv("SIMCORE_INSETS") ?: "0,0").split(',').map { it.trim().toFloatOrNull() ?: 0f }
        insetTop = parts.getOrElse(0) { 0f }
        insetBottom = parts.getOrElse(1) { 0f }
    }

    private var content: (@Composable () -> Unit)? = null

    fun setContent(c: @Composable () -> Unit) {
        content = c
    }

    private lateinit var wire: PrintStream
    private val root = LayoutNode("Box")
    private var version = 0L
    private var actions: Map<Long, () -> Unit> = emptyMap()
    private var inputs: Map<Long, (String) -> Unit> = emptyMap()
    private var lastTree = ""

    private fun send(line: String) {
        wire.print(line)
        wire.print('\n')
        wire.flush()
    }

    private fun error(message: String) = send("""{"type":"error","message":${androidx.compose.ui.jsonString(message)}}""")

    /** Serialize; with `force` a tree is sent even when unchanged (the host waits for one after each action). */
    private fun emit(force: Boolean) {
        val acts = HashMap<Long, () -> Unit>()
        val fields = HashMap<Long, (String) -> Unit>()
        val sb = StringBuilder()
        root.toJson("root", acts, fields, sb)
        val tree = sb.toString()
        // Callbacks can change while their wire IDs stay the same.
        actions = acts
        inputs = fields
        if (!force && tree == lastTree) return
        lastTree = tree
        version++
        send("""{"type":"tree","version":$version,"root":$tree}""")
    }

    @JvmStatic
    fun main(args: Array<String>) {
        // The protocol owns the real stdout; app println goes to stderr (the host's session log).
        wire = PrintStream(java.io.FileOutputStream(java.io.FileDescriptor.out), false, "UTF-8")
        System.setOut(System.err)
        val activityClass = args.firstOrNull() ?: run {
            System.err.println("usage: Guest <ComponentActivity class>")
            exitProcess(2)
        }
        val messages = Channel<String>(Channel.UNLIMITED)
        Thread({
            val r = System.`in`.bufferedReader()
            while (true) {
                val line = r.readLine() ?: break
                messages.trySend(line)
            }
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

            try {
                val activity = Class.forName(activityClass).getDeclaredConstructor().newInstance() as ComponentActivity
                activity.onCreate(null)
                activity.onStart()
                activity.onResume()
            } catch (t: Throwable) {
                val cause = (t as? java.lang.reflect.InvocationTargetException)?.targetException ?: t
                error("activity $activityClass failed to start: ${cause.stackTraceToString()}")
                exitProcess(1)
            }
            val app = content ?: run {
                error("$activityClass did not call setContent { }")
                exitProcess(1)
            }
            val composition = Composition(LayoutApplier(root), recomposer)
            composition.setContent(app)
            pump()
            emit(force = true)

            while (true) {
                val line = withContext(Dispatchers.IO) { messages.receiveCatching().getOrNull() } ?: break
                val type = Regex(""""type"\s*:\s*"(\w+)"""").find(line)?.groupValues?.get(1)
                fun num(key: String) = Regex(""""$key"\s*:\s*(\d+)""").find(line)?.groupValues?.get(1)?.toLong()
                when (type) {
                    "action", "text_input" -> {
                        val v = num("version")
                        val before = version
                        try {
                            if (v == version) {
                                if (type == "action") actions[num("id")]?.invoke()
                                else textInput(line)?.let { inputs[num("field")]?.invoke(it) }
                            }
                        } catch (t: Throwable) {
                            error("Input callback threw: ${t.stackTraceToString()}")
                        }
                        pump()
                        // The write observer may already have sent the new tree; otherwise send one (even unchanged).
                        emit(force = version == before)
                    }
                    "environment" -> {
                        Regex(""""appearance"\s*:\s*"(\w+)"""").find(line)?.groupValues?.get(1)?.let { dark.value = it == "dark" }
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

    private fun textInput(line: String): String? {
        val text = Regex("""\"text"\s*:\s*"((?:[^"\\]|\\.)*)"""").find(line)?.groupValues?.get(1) ?: return null
        return Regex("""\\(u[0-9a-fA-F]{4}|["\\/bfnrt])""").replace(text) { match ->
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
