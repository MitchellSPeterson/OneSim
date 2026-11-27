package androidx.compose.ui.graphics

import androidx.compose.runtime.Immutable

/** Pivot of scale and rotation as fractions of the layer's size (0.5, 0.5 = centre). */
@Immutable
data class TransformOrigin(val pivotFractionX: Float, val pivotFractionY: Float) {
    companion object {
        val Center = TransformOrigin(0.5f, 0.5f)
    }
}
