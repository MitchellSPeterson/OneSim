package androidx.compose.ui.text.font

import androidx.compose.runtime.Immutable
import kotlin.jvm.JvmInline

@Immutable
data class FontWeight(val weight: Int) : Comparable<FontWeight> {
    override fun compareTo(other: FontWeight) = weight.compareTo(other.weight)

    companion object {
        val Thin = FontWeight(100)
        val ExtraLight = FontWeight(200)
        val Light = FontWeight(300)
        val Normal = FontWeight(400)
        val Medium = FontWeight(500)
        val SemiBold = FontWeight(600)
        val Bold = FontWeight(700)
        val ExtraBold = FontWeight(800)
        val Black = FontWeight(900)
        val W100 = Thin
        val W200 = ExtraLight
        val W300 = Light
        val W400 = Normal
        val W500 = Medium
        val W600 = SemiBold
        val W700 = Bold
        val W800 = ExtraBold
        val W900 = Black
    }
}

@JvmInline
value class FontStyle(val value: Int) {
    companion object {
        val Normal = FontStyle(0)
        val Italic = FontStyle(1)
    }
}

/** Only the system families are rendered (as Roboto); custom fonts are not supported yet. */
open class FontFamily private constructor(val name: String) {
    companion object {
        val Default = FontFamily("default")
        val SansSerif = FontFamily("sans-serif")
        val Serif = FontFamily("serif")
        val Monospace = FontFamily("monospace")
        val Cursive = FontFamily("cursive")
    }
}
