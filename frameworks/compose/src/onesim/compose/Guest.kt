package onesim.compose

import androidx.activity.BackEventCompat
import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.AndroidViewConfiguration
import androidx.compose.ui.platform.ViewConfiguration
import androidx.compose.ui.unit.Density
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob

/**
 * Shared state and tree serialization for the Compose guest (JVM wire host and Kotlin/Wasm browser target).
 */
object Guest {
    /** `isSystemInDarkTheme()`; updated by the host environment. */
    val dark = mutableStateOf(false)
    private val screenWidthState = mutableStateOf(411.42856f)
    private val screenHeightState = mutableStateOf(923.4286f)

    /** Screen size in dp (`SIMCORE_VIEWPORT`; Pixel 10a 411 x 923), as `LocalConfiguration.screenWidthDp` reads it. */
    var screenWidth: Float
        get() = screenWidthState.value
        set(value) {
            screenWidthState.value = value
        }

    var screenHeight: Float
        get() = screenHeightState.value
        set(value) {
            screenHeightState.value = value
        }

    private val insetTopState = mutableStateOf(0f)
    private val insetBottomState = mutableStateOf(0f)

    var insetTop: Float
        get() = insetTopState.value
        set(value) {
            insetTopState.value = value
        }

    var insetBottom: Float
        get() = insetBottomState.value
        set(value) {
            insetBottomState.value = value
        }

    /** Device pixels per dp (`SIMCORE_SCALE`; Pixel 10a 2.625). Pointer input works in pixels. */
    var density: Float = 2.625f
        set(value) {
            field = value
            densityObject = Density(value)
            viewConfiguration = AndroidViewConfiguration(value)
        }
    var densityObject: Density = Density(density)
        private set
    var viewConfiguration: ViewConfiguration = AndroidViewConfiguration(density)
        private set

    /**
     * Scope of node coroutines (pointer input handlers, indications, drags): the guest loop's dispatcher and frame clock,
     * supervised so a throwing handler is reported instead of ending the app.
     */
    var scope: CoroutineScope = CoroutineScope(SupervisorJob())

    internal var logSink: (String) -> Unit = { println(it) }

    /** Guest → host lines other than trees (`{"type":"event",...}`). */
    var messageSink: (String) -> Unit = {}

    /** The running activity (system back goes to its `onBackPressedDispatcher`). */
    var activity: ComponentActivity? = null
        internal set

    /** Reports an exception thrown by app code outside composition (input handlers). */
    var errorSink: (String) -> Unit = { logSink(it) }

    internal var content: (@Composable () -> Unit)? = null
    val root = LayoutNode("Box")
    var version = 0L
        internal set
    private var actions: Map<Long, () -> Unit> = emptyMap()
    private var inputs: Map<Long, (String) -> Unit> = emptyMap()
    private var gestures: Map<String, PointerTarget> = emptyMap()
    private var live: Set<NodeState> = emptySet()
    private var lastTree = ""

    fun setContent(c: @Composable () -> Unit) {
        // Window-level layers (dialogs, modal rails) are drawn over the app's content.
        content = {
            c()
            OverlayHost()
        }
    }

    internal fun log(line: String) = logSink(line)

    internal fun reset() {
        content = null
        activity = null
        Overlays.entries.clear()
        root.children.clear()
        version = 0L
        actions = emptyMap()
        inputs = emptyMap()
        for (s in live) s.detachOnce()
        gestures = emptyMap()
        live = emptySet()
        Pointers.reset()
        lastTree = ""
    }

    fun startActivity(activity: ComponentActivity): @Composable () -> Unit {
        this.activity = activity
        activity.onCreate(null)
        activity.onStart()
        activity.onResume()
        return content ?: error("${activity::class.simpleName ?: "Activity"} did not call setContent { }")
    }

    /** Serialize the composed tree and refresh wire callback tables. Returns null if unchanged and `!force`. */
    fun serialize(force: Boolean): String? {
        val tables = Tables()
        val sb = StringBuilder()
        root.toJson("root", tables, sb)
        val tree = sb.toString()
        // Callbacks can change while their wire IDs stay the same.
        actions = tables.actions
        inputs = tables.inputs
        gestures = tables.gestures
        // Node state whose element or node left the tree.
        for (s in live) if (s !in tables.live) s.detachOnce()
        live = tables.live
        if (!force && tree == lastTree) return null
        lastTree = tree
        version++
        return tree
    }

    internal fun gestureTarget(key: String): PointerTarget? = gestures[key]

    fun dispatchAction(id: Long) {
        actions[id]?.invoke()
    }

    fun dispatchTextInput(field: Long, text: String) {
        inputs[field]?.invoke(text)
    }

    /** Called by `ComponentActivity.finish()`: the host is told the activity would close. */
    internal fun activityFinished() {
        log("activity finished (system back with no enabled OnBackPressedCallback)")
        messageSink("""{"type":"event","name":"activity.finished","detail":{}}""")
    }

    /**
     * One `back` message (protocol section X) to the activity's `OnBackPressedDispatcher`: a predictive back gesture's
     * start / progress / cancel / commit, or the back key (a lone commit).
     */
    fun dispatchBack(m: Map<*, *>) {
        val dispatcher = activity?.onBackPressedDispatcher ?: return
        val phase = m["phase"] as? String ?: return
        val edge = when (m["edge"]) {
            "left" -> BackEventCompat.EDGE_LEFT
            "right" -> BackEventCompat.EDGE_RIGHT
            else -> BackEventCompat.EDGE_NONE
        }
        val progress = ((m["progress"] as? Double) ?: 0.0).toFloat().coerceIn(0f, 1f)
        val x = ((m["x"] as? Double) ?: 0.0).toFloat() * density
        val y = ((m["y"] as? Double) ?: 0.0).toFloat() * density
        val event = BackEventCompat(x, y, progress, edge, ((m["time"] as? Double) ?: 0.0).toLong())
        when (phase) {
            "start" -> dispatcher.dispatchOnBackStarted(event)
            "progress" -> dispatcher.dispatchOnBackProgressed(event)
            "cancel" -> dispatcher.dispatchOnBackCancelled()
            "commit" -> dispatcher.onBackPressed()
        }
    }

    /** One `gesture` message (protocol section L): touches on the pointer input targets it names; a `back` message
     * (section X) arriving on the same channel (the web bridge) goes to [dispatchBack]. */
    fun dispatchGesture(json: String) {
        val m = Json.parse(json) as? Map<*, *> ?: return
        if (m["type"] == "back") return dispatchBack(m)
        val phase = m["phase"] as? String ?: return
        val time = ((m["time"] as? Double) ?: 0.0).toLong()
        val points = (m["points"] as? List<*>).orEmpty().mapNotNull { p ->
            val xy = p as? List<*> ?: return@mapNotNull null
            Offset(((xy.getOrNull(0) as? Double) ?: return@mapNotNull null).toFloat(), ((xy.getOrNull(1) as? Double) ?: return@mapNotNull null).toFloat())
        }
        val targets = (m["targets"] as? List<*>).orEmpty().mapNotNull { t ->
            val o = t as? Map<*, *> ?: return@mapNotNull null
            val key = o["key"] as? String ?: return@mapNotNull null
            val f = (o["frame"] as? List<*>).orEmpty().map { ((it as? Double) ?: 0.0).toFloat() }
            if (f.size < 4) null else key to f.toFloatArray()
        }
        Pointers.dispatch(phase, time, points, targets)
    }
}
