package onesim.compose

import androidx.compose.runtime.AbstractApplier
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ComposeNode
import androidx.compose.ui.ClickModifier
import androidx.compose.ui.Modifier
import androidx.compose.ui.WireModifier
import androidx.compose.ui.jsonString

/** One layout primitive in the composed tree (`Box`, `Column`, `Row`, `Text`). */
class LayoutNode(val component: String) {
    /** Props as JSON fragments (`"key":value`, comma-joined), without the modifier. */
    var props: String = ""
    var modifier: Modifier = Modifier
    var onInput: ((String) -> Unit)? = null
    var onTextClick: ((Int) -> Unit)? = null
    var isTextProbe: Boolean = false
    var onTextLayout: ((String) -> Unit)? = null
    val children = mutableListOf<LayoutNode>()

    /** Wire JSON of this subtree and the callbacks referenced by it. */
    internal fun toJson(id: String, handlers: CallbackTables, out: StringBuilder) {
        out.append("""{"kind":"compose","id":""").append(jsonString(id))
        out.append(""","component":"""").append(component).append("""","props":{""")
        var first = true
        if (props.isNotEmpty()) {
            out.append(props)
            first = false
        }
        onInput?.let { handler ->
            val field = handlers.inputs.size + 1L
            handlers.inputs[field] = handler
            if (!first) out.append(',')
            out.append("\"field\":").append(field)
            first = false
        }
        onTextClick?.let { handler ->
            val action = handlers.textClicks.size + 1L
            handlers.textClicks[action] = handler
            if (!first) out.append(',')
            out.append("\"textClick\":").append(action)
            first = false
        }
        onTextLayout?.let { handler ->
            val callback = if (isTextProbe) ++handlers.textProbeId else ++handlers.textLayoutId
            handlers.textLayouts[callback] = handler
            if (!first) out.append(',')
            out.append("\"textLayout\":").append(callback)
            first = false
        }
        val mods = modifier.foldIn(mutableListOf<String>()) { acc, e ->
            when (e) {
                is WireModifier -> acc.add(e.json)
                is ClickModifier -> {
                    val actionId = handlers.actions.size + 1L
                    handlers.actions[actionId] = e.onClick
                    val role = e.role?.let { ""","role":"$it"""" } ?: ""
                    val label = e.label?.let { ""","label":${jsonString(it)}""" } ?: ""
                    acc.add("""{"clickable":{"action":$actionId,"enabled":${e.enabled}$role$label}}""")
                }
            }
            acc
        }
        if (mods.isNotEmpty()) {
            if (!first) out.append(',')
            out.append("\"modifier\":[").append(mods.joinToString(",")).append(']')
        }
        out.append('}')
        if (children.isNotEmpty()) {
            out.append(""","children":[""")
            children.forEachIndexed { i, c ->
                if (i > 0) out.append(',')
                c.toJson("$id/$i", handlers, out)
            }
            out.append(']')
        }
        out.append('}')
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
    EmitPrimitive(component, modifier, props, onInput, null, null, content)
}

@Composable
fun TextPrimitive(
    modifier: Modifier, props: String, onClick: ((Int) -> Unit)?, onLayout: ((String) -> Unit)?,
    isProbe: Boolean = false,
    content: @Composable () -> Unit = {},
) {
    EmitPrimitive("Text", modifier, props, null, onClick, onLayout, content, isTextProbe = isProbe)
}

@Composable
private fun EmitPrimitive(
    component: String,
    modifier: Modifier,
    props: String,
    onInput: ((String) -> Unit)?,
    onTextClick: ((Int) -> Unit)?,
    onTextLayout: ((String) -> Unit)?,
    content: @Composable () -> Unit,
    isTextProbe: Boolean = false,
) {
    ComposeNode<LayoutNode, LayoutApplier>(
        factory = { LayoutNode(component) },
        update = {
            set(modifier) { this.modifier = it }
            set(props) { this.props = it }
            set(onInput) { this.onInput = it }
            set(onTextClick) { this.onTextClick = it }
            set(onTextLayout) { this.onTextLayout = it }
            set(isTextProbe) { this.isTextProbe = it }
        },
        content = content,
    )
}
