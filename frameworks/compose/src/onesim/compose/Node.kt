package onesim.compose

import androidx.compose.runtime.AbstractApplier
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ComposeNode
import androidx.compose.runtime.CompositionLocal
import androidx.compose.runtime.CompositionLocalMap
import androidx.compose.runtime.currentComposer
import androidx.compose.ui.Modifier
import androidx.compose.ui.WireModifier
import androidx.compose.ui.jsonString
import kotlin.reflect.KClass

/**
 * State a modifier element keeps on its layout node across recompositions (AndroidX's `Modifier.Node`): kept while an
 * element of the same class sits at the same position among the node's stateful elements, dropped ([detach]) when it
 * leaves or the node leaves the tree.
 */
abstract class NodeState {
    internal var attached = false

    open fun detach() {}
}

/** A modifier element with per-node state; it writes its wire entries when the tree is sent. */
interface NodeElement : Modifier.Element {
    fun create(): NodeState

    fun wire(state: NodeState, cx: WireCx)
}

/** Serialization context of one stateful element. */
class WireCx internal constructor(
    val node: LayoutNode,
    /** Structural key of this element: the node's path and the element's position in its chain. */
    val key: String,
    private val out: MutableList<String>,
    private val tables: Tables,
    private val container: BooleanArray,
) {
    /** Inside a scrollable container (an ancestor or an outer modifier of this node is `scrollable`). */
    val inScrollableContainer: Boolean get() = tables.scrollDepth > 0 || container[0]

    /** This element makes the node a scrollable container for what it wraps and for its children. */
    fun markScrollContainer() {
        container[0] = true
    }

    /** Add a wire modifier entry (JSON object text) at this position of the chain. */
    fun add(json: String) {
        out.add(json)
    }

    /** Register a callback the host can run as a section B `action`; returns its id. */
    fun action(onClick: () -> Unit): Long {
        val id = tables.actions.size + 1L
        tables.actions[id] = onClick
        return id
    }

    /** Register a pointer input target under this element's key (protocol section L `gesture` touches). */
    fun gesture(target: PointerTarget, suffix: String = ""): String {
        val k = key + suffix
        tables.gestures[k] = target
        return k
    }

    /** A composition local's value at the node, as AndroidX nodes read it with `currentValueOf`. */
    fun <T> local(key: CompositionLocal<T>): T = node.locals[key]
}

internal class Tables {
    val actions = HashMap<Long, () -> Unit>()
    val inputs = HashMap<Long, (String) -> Unit>()
    val gestures = HashMap<String, PointerTarget>()
    val live = HashSet<NodeState>()
    var scrollDepth = 0
}

/** One layout primitive in the composed tree (`Box`, `Column`, `Row`, `Text`). */
class LayoutNode(val component: String) {
    /** Props as JSON fragments (`"key":value`, comma-joined), without the modifier. */
    var props: String = ""
    var modifier: Modifier = Modifier
    var onInput: ((String) -> Unit)? = null
    var locals: CompositionLocalMap = CompositionLocalMap.Empty
    val children = mutableListOf<LayoutNode>()
    private val states = ArrayList<Pair<KClass<*>, NodeState>>()

    /** The state of the `index`-th stateful element, kept while the element class there stays the same. */
    private fun stateFor(index: Int, element: NodeElement): NodeState {
        val kind = element::class
        if (index < states.size) {
            val (k, s) = states[index]
            if (k == kind) return s
            s.detachOnce()
            states[index] = kind to element.create()
        } else {
            states.add(kind to element.create())
        }
        return states[index].second
    }

    /** Wire JSON of this subtree; callbacks are registered in [tables]. */
    internal fun toJson(id: String, tables: Tables, out: StringBuilder) {
        out.append("""{"kind":"compose","id":""").append(jsonString(id))
        out.append(""","component":"""").append(component).append("""","props":{""")
        var first = true
        if (props.isNotEmpty()) {
            out.append(props)
            first = false
        }
        onInput?.let { handler ->
            val field = tables.inputs.size + 1L
            tables.inputs[field] = handler
            if (!first) out.append(',')
            out.append("\"field\":").append(field)
            first = false
        }
        val mods = mutableListOf<String>()
        var stateful = 0
        var position = 0
        val container = booleanArrayOf(false)
        modifier.foldIn(Unit) { _, e ->
            when (e) {
                is WireModifier -> mods.add(e.json)
                is NodeElement -> {
                    val state = stateFor(stateful++, e)
                    state.attached = true
                    tables.live.add(state)
                    e.wire(state, WireCx(this, "$id#m$position", mods, tables, container))
                }
            }
            position++
        }
        while (states.size > stateful) states.removeAt(states.size - 1).second.detachOnce()
        if (mods.isNotEmpty()) {
            if (!first) out.append(',')
            out.append("\"modifier\":[").append(mods.joinToString(",")).append(']')
        }
        out.append('}')
        if (children.isNotEmpty()) {
            out.append(""","children":[""")
            if (container[0]) tables.scrollDepth++
            children.forEachIndexed { i, c ->
                if (i > 0) out.append(',')
                c.toJson("$id/$i", tables, out)
            }
            if (container[0]) tables.scrollDepth--
            out.append(']')
        }
        out.append('}')
    }
}

internal fun NodeState.detachOnce() {
    if (attached) {
        attached = false
        detach()
    }
}

class LayoutApplier(root: LayoutNode) : AbstractApplier<LayoutNode>(root) {
    override fun insertTopDown(index: Int, instance: LayoutNode) {
        current.children.add(index, instance)
    }

    override fun insertBottomUp(index: Int, instance: LayoutNode) {}

    override fun remove(index: Int, count: Int) {
        repeat(count) { current.children.removeAt(index) }
    }

    override fun move(from: Int, to: Int, count: Int) {
        current.children.move(from, to, count)
    }

    override fun onClear() {
        root.children.clear()
    }
}

/** Emit a layout primitive. `props` is a comma-joined list of JSON members. */
@Composable
fun Primitive(component: String, modifier: Modifier, props: String, content: @Composable () -> Unit = {}) {
    InputPrimitive(component, modifier, props, null, content)
}

@Composable
fun InputPrimitive(
    component: String,
    modifier: Modifier,
    props: String,
    onInput: ((String) -> Unit)?,
    content: @Composable () -> Unit = {},
) {
    val locals = currentComposer.currentCompositionLocalMap
    ComposeNode<LayoutNode, LayoutApplier>(
        factory = { LayoutNode(component) },
        update = {
            set(modifier) { this.modifier = it }
            set(props) { this.props = it }
            set(onInput) { this.onInput = it }
            set(locals) { this.locals = it }
        },
        content = content,
    )
}
