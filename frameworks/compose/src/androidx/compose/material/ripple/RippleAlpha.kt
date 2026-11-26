package androidx.compose.material.ripple

import androidx.compose.runtime.Immutable

/** Opacity of a ripple's state layers and of the ripple itself. */
@Immutable
class RippleAlpha(
    val draggedAlpha: Float,
    val focusedAlpha: Float,
    val hoveredAlpha: Float,
    val pressedAlpha: Float,
) {
    override fun equals(other: Any?) = other is RippleAlpha && draggedAlpha == other.draggedAlpha &&
        focusedAlpha == other.focusedAlpha && hoveredAlpha == other.hoveredAlpha && pressedAlpha == other.pressedAlpha
    override fun hashCode() = ((draggedAlpha.hashCode() * 31 + focusedAlpha.hashCode()) * 31 + hoveredAlpha.hashCode()) * 31 + pressedAlpha.hashCode()
    override fun toString() = "RippleAlpha(draggedAlpha=$draggedAlpha, focusedAlpha=$focusedAlpha, hoveredAlpha=$hoveredAlpha, pressedAlpha=$pressedAlpha)"
}
