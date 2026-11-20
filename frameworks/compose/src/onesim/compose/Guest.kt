package onesim.compose

import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf

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

    internal var logSink: (String) -> Unit = { println(it) }

    internal var content: (@Composable () -> Unit)? = null
    val root = LayoutNode("Box")
    var version = 0L
        internal set
    private var actions: Map<Long, () -> Unit> = emptyMap()
    private var inputs: Map<Long, (String) -> Unit> = emptyMap()
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
        val acts = HashMap<Long, () -> Unit>()
        val fields = HashMap<Long, (String) -> Unit>()
        val sb = StringBuilder()
        root.toJson("root", acts, fields, sb)
        val tree = sb.toString()
        // Callbacks can change while their wire IDs stay the same.
        actions = acts
        inputs = fields
        if (!force && tree == lastTree) return null
        lastTree = tree
        version++
        return tree
    }

    fun dispatchAction(id: Long) {
        actions[id]?.invoke()
    }

    fun dispatchTextInput(field: Long, text: String) {
        inputs[field]?.invoke(text)
    }
}
