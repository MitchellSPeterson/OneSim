package androidx.compose.foundation.selection

import androidx.compose.foundation.ClickableElement
import androidx.compose.foundation.Indication
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role

/** A clickable that flips [value]: [onValueChange] gets `!value`. Draws `LocalIndication` while pressed. */
fun Modifier.toggleable(
    value: Boolean,
    enabled: Boolean = true,
    role: Role? = null,
    interactionSource: MutableInteractionSource? = null,
    onValueChange: (Boolean) -> Unit,
): Modifier = this then ClickableElement(interactionSource, null, true, enabled, null, role, { onValueChange(!value) })

/** [toggleable] with an explicit [indication] (`null`: none). */
fun Modifier.toggleable(
    value: Boolean,
    interactionSource: MutableInteractionSource?,
    indication: Indication?,
    enabled: Boolean = true,
    role: Role? = null,
    onValueChange: (Boolean) -> Unit,
): Modifier = this then ClickableElement(interactionSource, indication, false, enabled, null, role, { onValueChange(!value) })

/** A clickable option of a group (radio button, tab): [onClick] selects it. Draws `LocalIndication` while pressed. */
fun Modifier.selectable(
    selected: Boolean,
    enabled: Boolean = true,
    role: Role? = null,
    interactionSource: MutableInteractionSource? = null,
    onClick: () -> Unit,
): Modifier = this then ClickableElement(interactionSource, null, true, enabled, null, role, onClick)

/** [selectable] with an explicit [indication] (`null`: none). */
fun Modifier.selectable(
    selected: Boolean,
    interactionSource: MutableInteractionSource?,
    indication: Indication?,
    enabled: Boolean = true,
    role: Role? = null,
    onClick: () -> Unit,
): Modifier = this then ClickableElement(interactionSource, indication, false, enabled, null, role, onClick)

/**
 * Marks a group of [selectable] items (a radio group) for accessibility services. It has no visible effect and the
 * host has no accessibility tree beyond roles and labels, so the wire carries nothing.
 */
fun Modifier.selectableGroup(): Modifier = this
