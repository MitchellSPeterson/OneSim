package androidx.compose.ui

import androidx.compose.runtime.Immutable

/** Biases -1 (start/top) ... 1 (end/bottom), as in `BiasAlignment`. */
@Immutable
interface Alignment {
    val horizontalBias: Float
    val verticalBias: Float

    @Immutable
    interface Horizontal { val bias: Float }

    @Immutable
    interface Vertical { val bias: Float }

    companion object {
        val TopStart: Alignment = BiasAlignment(-1f, -1f)
        val TopCenter: Alignment = BiasAlignment(0f, -1f)
        val TopEnd: Alignment = BiasAlignment(1f, -1f)
        val CenterStart: Alignment = BiasAlignment(-1f, 0f)
        val Center: Alignment = BiasAlignment(0f, 0f)
        val CenterEnd: Alignment = BiasAlignment(1f, 0f)
        val BottomStart: Alignment = BiasAlignment(-1f, 1f)
        val BottomCenter: Alignment = BiasAlignment(0f, 1f)
        val BottomEnd: Alignment = BiasAlignment(1f, 1f)

        val Top: Vertical = BiasAlignment.Vertical(-1f)
        val CenterVertically: Vertical = BiasAlignment.Vertical(0f)
        val Bottom: Vertical = BiasAlignment.Vertical(1f)

        val Start: Horizontal = BiasAlignment.Horizontal(-1f)
        val CenterHorizontally: Horizontal = BiasAlignment.Horizontal(0f)
        val End: Horizontal = BiasAlignment.Horizontal(1f)
    }
}

@Immutable
data class BiasAlignment(override val horizontalBias: Float, override val verticalBias: Float) : Alignment {
    @Immutable
    data class Horizontal(override val bias: Float) : Alignment.Horizontal

    @Immutable
    data class Vertical(override val bias: Float) : Alignment.Vertical
}
