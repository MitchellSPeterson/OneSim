package onesim.compose

import androidx.activity.ComponentActivity
import androidx.compose.runtime.BroadcastFrameClock
import androidx.compose.runtime.Composition
import androidx.compose.runtime.Recomposer
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
 * JVM Compose guest entrypoint (protocol sections B and K): runs the app's composition on the real Compose runtime
 * with a [LayoutApplier], sends trees as JSON lines on the original stdout, and applies host messages from stdin.
 */
object JvmGuest {
    private lateinit var wire: PrintStream

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
        Guest.dark.value = System.getenv("SIMCORE_APPEARANCE") == "dark"
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

            while (true) {
                val line = withContext(Dispatchers.IO) { messages.receiveCatching().getOrNull() } ?: break
                val type = Regex(""""type"\s*:\s*"(\w+)"""").find(line)?.groupValues?.get(1)
                fun num(key: String) = Regex(""""$key"\s*:\s*(\d+)""").find(line)?.groupValues?.get(1)?.toLong()
                when (type) {
                    "action", "text_input" -> {
                        val v = num("version")
                        val before = Guest.version
                        try {
                            if (v == Guest.version) {
                                if (type == "action") num("id")?.let { Guest.dispatchAction(it) }
                                else textInput(line)?.let { text -> num("field")?.let { Guest.dispatchTextInput(it, text) } }
                            }
                        } catch (t: Throwable) {
                            error("Input callback threw: ${t.stackTraceToString()}")
                        }
                        pump()
                        // The write observer may already have sent the new tree; otherwise send one (even unchanged).
                        emit(force = Guest.version == before)
                    }
                    "environment" -> {
                        Regex(""""appearance"\s*:\s*"(\w+)"""").find(line)?.groupValues?.get(1)?.let { Guest.dark.value = it == "dark" }
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
