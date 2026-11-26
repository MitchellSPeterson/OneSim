package androidx.compose.foundation

import androidx.compose.foundation.gestures.PressGestureScope
import androidx.compose.foundation.gestures.detectTapAndPress
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.PointerInputEventHandler
import androidx.compose.ui.jsonString
import androidx.compose.ui.semantics.Role
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import onesim.compose.NodeElement
import onesim.compose.NodeState
import onesim.compose.PointerTarget
import onesim.compose.WireCx

/** How long a press inside a scrollable container waits before showing (Android's tap timeout). */
internal const val TapIndicationDelay: Long = 100L

/**
 * Tap target: [onClick] runs when a finger lifts inside the node after going down on it. While pressed it emits
 * [PressInteraction]s to [interactionSource] and draws [LocalIndication] (Material 3: a ripple).
 */
fun Modifier.clickable(
    enabled: Boolean = true,
    onClickLabel: String? = null,
    role: Role? = null,
    interactionSource: MutableInteractionSource? = null,
    onClick: () -> Unit,
): Modifier = this then ClickableElement(interactionSource, null, true, enabled, onClickLabel, role, onClick)

/** [clickable] with an explicit [indication] (`null`: none). */
fun Modifier.clickable(
    interactionSource: MutableInteractionSource?,
    indication: Indication?,
    enabled: Boolean = true,
    onClickLabel: String? = null,
    role: Role? = null,
    onClick: () -> Unit,
): Modifier = this then ClickableElement(interactionSource, indication, false, enabled, onClickLabel, role, onClick)

/**
 * [clickable] that also reports long clicks (after the long press timeout) and double clicks. With [onDoubleClick],
 * [onClick] waits for the double tap timeout to pass.
 */
fun Modifier.combinedClickable(
    enabled: Boolean = true,
    onClickLabel: String? = null,
    role: Role? = null,
    onLongClickLabel: String? = null,
    onLongClick: (() -> Unit)? = null,
    onDoubleClick: (() -> Unit)? = null,
    hapticFeedbackEnabled: Boolean = true,
    interactionSource: MutableInteractionSource? = null,
    onClick: () -> Unit,
): Modifier = this then ClickableElement(interactionSource, null, true, enabled, onClickLabel, role, onClick, onLongClick, onDoubleClick)

/** [combinedClickable] with an explicit [indication]. */
fun Modifier.combinedClickable(
    interactionSource: MutableInteractionSource?,
    indication: Indication?,
    enabled: Boolean = true,
    onClickLabel: String? = null,
    role: Role? = null,
    onLongClickLabel: String? = null,
    onLongClick: (() -> Unit)? = null,
    onDoubleClick: (() -> Unit)? = null,
    hapticFeedbackEnabled: Boolean = true,
    onClick: () -> Unit,
): Modifier = this then ClickableElement(interactionSource, indication, false, enabled, onClickLabel, role, onClick, onLongClick, onDoubleClick)

/**
 * The clickable family (`clickable`, `combinedClickable`, `toggleable`, `selectable`): a `clickable` wire entry carrying
 * the semantics and a gesture key, then the indication's ripple.
 */
internal class ClickableElement(
    val interactionSource: MutableInteractionSource?,
    val indication: Indication?,
    /** Read [LocalIndication] at the node (overloads without an `indication` parameter). */
    val useLocalIndication: Boolean,
    val enabled: Boolean,
    val onClickLabel: String?,
    val role: Role?,
    val onClick: () -> Unit,
    val onLongClick: (() -> Unit)? = null,
    val onDoubleClick: (() -> Unit)? = null,
) : NodeElement {
    override fun create(): NodeState = ClickableState()

    override fun wire(state: NodeState, cx: WireCx) = (state as ClickableState).wire(this, cx)
}

internal class ClickableState : NodeState() {
    private lateinit var element: ClickableElement
    private var internalSource: MutableInteractionSource? = null
    private val indication = IndicationHolder()
    private var press: PressInteraction.Press? = null
    private var pressSource: MutableInteractionSource? = null
    private var inScrollable = false
    private var shape: Triple<Boolean, Boolean, Boolean>? = null
    private var currentSource: MutableInteractionSource? = null

    private val target = PointerTarget(PointerInputEventHandler { })

    /** The source this clickable emits into: the given one, or its own when it draws an indication. */
    private fun source(indication: Indication?): MutableInteractionSource? =
        element.interactionSource ?: if (indication != null) (internalSource ?: MutableInteractionSource().also { internalSource = it }) else null

    fun wire(e: ClickableElement, cx: WireCx) {
        element = e
        inScrollable = cx.inScrollableContainer
        val indication = if (e.useLocalIndication) cx.local(LocalIndication) else e.indication
        val source = source(indication)
        if (source !== currentSource || !e.enabled) cancelPress()
        currentSource = source
        // The gesture detector depends on which callbacks exist (AndroidX resets pointer input when they change).
        val newShape = Triple(e.enabled, e.onLongClick != null, e.onDoubleClick != null)
        if (newShape != shape) {
            shape = newShape
            target.reset()
            target.handler = handler(e.onLongClick != null, e.onDoubleClick != null)
        }
        val role = e.role?.let { ""","role":"$it"""" } ?: ""
        val key = if (e.enabled) ""","key":${jsonString(cx.gesture(target))}""" else ""
        val action = cx.action { if (element.enabled) element.onClick() }
        cx.add("""{"clickable":{"action":$action,"enabled":${e.enabled}$role$key}}""")
        this.indication.wire(indication, source, cx)
    }

    private fun handler(long: Boolean, double: Boolean) = PointerInputEventHandler {
        if (!long && !double) {
            detectTapAndPress(onPress = { offset -> if (element.enabled) handlePress(offset) }, onTap = { if (element.enabled) element.onClick() })
        } else {
            detectTapGestures(
                onDoubleTap = if (double) { _ -> if (element.enabled) element.onDoubleClick?.invoke() } else null,
                onLongPress = if (long) { _ -> if (element.enabled) element.onLongClick?.invoke() } else null,
                onPress = { offset -> if (element.enabled) handlePress(offset) },
                onTap = { if (element.enabled) element.onClick() },
            )
        }
    }

    /** AndroidX `handlePressInteraction`: press now (or after the tap timeout inside a scroller), then release or cancel. */
    private suspend fun PressGestureScope.handlePress(position: Offset) {
        val source = currentSource ?: run {
            tryAwaitRelease()
            return
        }
        coroutineScope {
            val delayJob = launch {
                if (inScrollable) delay(TapIndicationDelay)
                val p = PressInteraction.Press(position)
                source.emit(p)
                press = p
                pressSource = source
            }
            val success = tryAwaitRelease()
            if (delayJob.isActive) {
                delayJob.cancel()
                if (success) {
                    // Released before the press showed: show it now so the ripple still plays.
                    val p = PressInteraction.Press(position)
                    source.emit(p)
                    source.emit(PressInteraction.Release(p))
                }
            } else {
                press?.let { p -> source.emit(if (success) PressInteraction.Release(p) else PressInteraction.Cancel(p)) }
            }
            press = null
            pressSource = null
        }
    }

    private fun cancelPress() {
        val p = press ?: return
        pressSource?.tryEmit(PressInteraction.Cancel(p))
        press = null
        pressSource = null
    }

    override fun detach() {
        cancelPress()
        target.reset()
        shape = null
        indication.detach()
    }
}
