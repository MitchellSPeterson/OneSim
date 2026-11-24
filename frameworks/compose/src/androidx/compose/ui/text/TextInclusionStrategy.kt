package androidx.compose.ui.text

import androidx.compose.ui.geometry.Rect
import kotlin.jvm.JvmInline

@JvmInline
value class TextGranularity private constructor(private val value: Int) {
    companion object {
        val Character = TextGranularity(0)
        val Word = TextGranularity(1)
    }
}

fun interface TextInclusionStrategy {
    fun isIncluded(textBounds: Rect, rect: Rect): Boolean

    companion object {
        val AnyOverlap = TextInclusionStrategy { bounds, rect ->
            bounds.left < rect.right && rect.left < bounds.right && bounds.top < rect.bottom && rect.top < bounds.bottom
        }
        val ContainsAll = TextInclusionStrategy { bounds, rect ->
            rect.width > 0 && rect.height > 0 && rect.left <= bounds.left && rect.top <= bounds.top &&
                rect.right >= bounds.right && rect.bottom >= bounds.bottom
        }
        val ContainsCenter = TextInclusionStrategy { bounds, rect ->
            val center = bounds.center
            rect.left <= center.x && center.x < rect.right && rect.top <= center.y && center.y < rect.bottom
        }
    }
}
