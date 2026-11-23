package onesim.compose

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
        content = c
    }

    internal fun log(line: String) = logSink(line)

    internal fun reset() {
        content = null
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

    /** One `gesture` message (protocol section L): touches on the pointer input targets it names. */
    fun dispatchGesture(json: String) {
        val m = Json.parse(json) as? Map<*, *> ?: return
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
