package androidx.compose.foundation.interaction

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.Stable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.geometry.Offset
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow

/** Something a user did to a component: pressed, dragged, hovered or focused it. */
interface Interaction

/** A stream of [Interaction]s, read by indications and `collectIs*AsState`. */
@Stable
interface InteractionSource {
    val interactions: Flow<Interaction>
}

/** An [InteractionSource] components emit into. */
@Stable
interface MutableInteractionSource : InteractionSource {
    suspend fun emit(interaction: Interaction)

    /** Emit without suspending; false when the buffer is full. */
    fun tryEmit(interaction: Interaction): Boolean
}

fun MutableInteractionSource(): MutableInteractionSource = MutableInteractionSourceImpl()

@Stable
private class MutableInteractionSourceImpl : MutableInteractionSource {
    override val interactions = MutableSharedFlow<Interaction>(extraBufferCapacity = 16, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    override suspend fun emit(interaction: Interaction) = interactions.emit(interaction)

    override fun tryEmit(interaction: Interaction) = interactions.tryEmit(interaction)
}

object PressInteraction {
    /** A press at [pressPosition] (pixels, local to the pressed node). */
    class Press(val pressPosition: Offset) : Interaction

    class Release(val press: Press) : Interaction

    class Cancel(val press: Press) : Interaction
}

object DragInteraction {
    class Start : Interaction

    class Stop(val start: Start) : Interaction

    class Cancel(val start: Start) : Interaction
}

object HoverInteraction {
    class Enter : Interaction

    class Exit(val enter: Enter) : Interaction
}

object FocusInteraction {
    class Focus : Interaction

    class Unfocus(val focus: Focus) : Interaction
}

/** True while this source has a press that was neither released nor cancelled. */
@Composable
fun InteractionSource.collectIsPressedAsState(): State<Boolean> {
    val isPressed = remember { mutableStateOf(false) }
    LaunchedEffect(this) {
        val presses = mutableListOf<PressInteraction.Press>()
        interactions.collect { interaction ->
            when (interaction) {
                is PressInteraction.Press -> presses.add(interaction)
                is PressInteraction.Release -> presses.remove(interaction.press)
                is PressInteraction.Cancel -> presses.remove(interaction.press)
            }
            isPressed.value = presses.isNotEmpty()
        }
    }
    return isPressed
}

/** True while this source has a drag that was neither stopped nor cancelled. */
@Composable
fun InteractionSource.collectIsDraggedAsState(): State<Boolean> {
    val isDragged = remember { mutableStateOf(false) }
    LaunchedEffect(this) {
        val drags = mutableListOf<DragInteraction.Start>()
        interactions.collect { interaction ->
            when (interaction) {
                is DragInteraction.Start -> drags.add(interaction)
                is DragInteraction.Stop -> drags.remove(interaction.start)
                is DragInteraction.Cancel -> drags.remove(interaction.start)
            }
            isDragged.value = drags.isNotEmpty()
        }
    }
    return isDragged
}

/** True while a pointer hovers (never on a touch screen). */
@Composable
fun InteractionSource.collectIsHoveredAsState(): State<Boolean> {
    val isHovered = remember { mutableStateOf(false) }
    LaunchedEffect(this) {
        val hovers = mutableListOf<HoverInteraction.Enter>()
        interactions.collect { interaction ->
            when (interaction) {
                is HoverInteraction.Enter -> hovers.add(interaction)
                is HoverInteraction.Exit -> hovers.remove(interaction.enter)
            }
            isHovered.value = hovers.isNotEmpty()
        }
    }
    return isHovered
}

/** True while the component has focus. */
@Composable
fun InteractionSource.collectIsFocusedAsState(): State<Boolean> {
    val isFocused = remember { mutableStateOf(false) }
    LaunchedEffect(this) {
        val focuses = mutableListOf<FocusInteraction.Focus>()
        interactions.collect { interaction ->
            when (interaction) {
                is FocusInteraction.Focus -> focuses.add(interaction)
                is FocusInteraction.Unfocus -> focuses.remove(interaction.focus)
            }
            isFocused.value = focuses.isNotEmpty()
        }
    }
    return isFocused
}
