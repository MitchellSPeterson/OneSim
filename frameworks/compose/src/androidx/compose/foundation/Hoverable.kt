package androidx.compose.foundation

import androidx.compose.foundation.interaction.HoverInteraction
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.ui.Modifier
import onesim.compose.NodeElement
import onesim.compose.NodeState
import onesim.compose.WireCx

/**
 * Emits [HoverInteraction.Enter] / [HoverInteraction.Exit] to [interactionSource] while a mouse or stylus pointer hovers
 * over the node. The simulated Pixel is a touch screen: fingers never hover, so no hover interactions occur (as on the
 * device). Disabling it or leaving the composition ends a hover in progress with an exit.
 */
fun Modifier.hoverable(interactionSource: MutableInteractionSource, enabled: Boolean = true): Modifier =
    this then HoverableElement(interactionSource, enabled)

private class HoverableElement(val source: MutableInteractionSource, val enabled: Boolean) : NodeElement {
    override fun create(): NodeState = HoverState()

    override fun wire(state: NodeState, cx: WireCx) {
        val s = state as HoverState
        if (!enabled || s.source !== source) s.exit()
        s.source = source
    }

    override fun equals(other: Any?) = other is HoverableElement && source == other.source && enabled == other.enabled
    override fun hashCode() = source.hashCode() * 31 + enabled.hashCode()
}

private class HoverState : NodeState() {
    var source: MutableInteractionSource? = null
    private var hover: HoverInteraction.Enter? = null

    fun exit() {
        val h = hover ?: return
        source?.tryEmit(HoverInteraction.Exit(h))
        hover = null
    }

    override fun detach() = exit()
}
