package androidx.compose.foundation

import androidx.compose.foundation.interaction.FocusInteraction
import androidx.compose.foundation.interaction.HoverInteraction
import androidx.compose.foundation.interaction.Interaction
import androidx.compose.foundation.interaction.InteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.runtime.Stable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.jsonNum
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import onesim.compose.Guest
import onesim.compose.NodeElement
import onesim.compose.NodeState
import onesim.compose.WireCx
import onesim.compose.detachOnce

/**
 * Visual feedback for interactions (a ripple, a pressed overlay), drawn over the content of the node it is applied
 * to. OneSim draws the indications it implements itself: [LocalIndication]'s default, Material 3's `ripple()` and
 * `null`. Other implementations are accepted and draw nothing.
 */
@Stable
interface Indication

/** OneSim's drawing of an [Indication]: per-node state that follows an [InteractionSource] and writes wire entries. */
internal interface IndicationRenderer : Indication {
    fun createState(): IndicationNodeState
}

internal abstract class IndicationNodeState : NodeState() {
    abstract fun bind(source: InteractionSource)

    abstract fun wire(cx: WireCx)
}

/** The indication components use when none is given; Material 3's `MaterialTheme` provides `ripple()`. */
val LocalIndication = staticCompositionLocalOf<Indication> { DefaultDebugIndication }

/** Foundation's default indication: a dark overlay while pressed (30%) or hovered/focused (10%). */
private object DefaultDebugIndication : IndicationRenderer {
    override fun createState() = OverlayState()

    class OverlayState : IndicationNodeState() {
        private var job: Job? = null
        private var source: InteractionSource? = null
        private val pressed = mutableStateOf(0)
        private val other = mutableStateOf(0)

        override fun bind(source: InteractionSource) {
            if (source === this.source) return
            job?.cancel()
            pressed.value = 0
            other.value = 0
            this.source = source
            job = Guest.scope.launch {
                val presses = HashSet<Interaction>()
                val rest = HashSet<Interaction>()
                source.interactions.collect { i ->
                    when (i) {
                        is PressInteraction.Press -> presses.add(i)
                        is PressInteraction.Release -> presses.remove(i.press)
                        is PressInteraction.Cancel -> presses.remove(i.press)
                        is HoverInteraction.Enter, is FocusInteraction.Focus -> rest.add(i)
                        is HoverInteraction.Exit -> rest.remove(i.enter)
                        is FocusInteraction.Unfocus -> rest.remove(i.focus)
                    }
                    pressed.value = presses.size
                    other.value = rest.size
                }
            }
        }

        override fun wire(cx: WireCx) {
            val alpha = if (pressed.value > 0) 0.3f else if (other.value > 0) 0.1f else 0f
            if (alpha > 0f) cx.add("""{"ripple":{"color":[0,0,0,1],"bounded":true,"layer":${jsonNum(alpha)}}}""")
        }

        override fun detach() {
            job?.cancel()
            job = null
            source = null
        }
    }
}

/** Holds the renderer state of whichever indication an element currently has. */
internal class IndicationHolder : NodeState() {
    private var indication: Indication? = null
    private var state: IndicationNodeState? = null

    /** Draw [indication] for [source] at this position of the chain. */
    fun wire(indication: Indication?, source: InteractionSource?, cx: WireCx) {
        if (indication != this.indication) {
            state?.detachOnce()
            state = (indication as? IndicationRenderer)?.createState()?.also { it.attached = true }
            this.indication = indication
        }
        val s = state ?: return
        if (source != null) {
            s.bind(source)
            s.wire(cx)
        }
    }

    override fun detach() {
        state?.detachOnce()
        state = null
        indication = null
    }
}

/** Draw [indication] for the interactions of [interactionSource] over this node's content. */
fun Modifier.indication(interactionSource: InteractionSource, indication: Indication?): Modifier =
    this then IndicationElement(interactionSource, indication)

private class IndicationElement(val source: InteractionSource, val indication: Indication?) : NodeElement {
    override fun create(): NodeState = IndicationHolder()

    override fun wire(state: NodeState, cx: WireCx) = (state as IndicationHolder).wire(indication, source, cx)

    override fun equals(other: Any?) = other is IndicationElement && source == other.source && indication == other.indication
    override fun hashCode() = source.hashCode() * 31 + indication.hashCode()
}
