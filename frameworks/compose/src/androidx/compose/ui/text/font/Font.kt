package androidx.compose.ui.text.font

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.State
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextLayoutInput
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.requestTextMeasurement
import androidx.compose.ui.text.style.TextOverflow
import kotlin.jvm.JvmInline
import onesim.compose.Guest

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

/** Generic families use the host's sans-serif, serif, monospace, and cursive fonts. */
data class Font(
    internal val resId: Int,
    val weight: FontWeight = FontWeight.Normal,
    val style: FontStyle = FontStyle.Normal,
    val loadingStrategy: FontLoadingStrategy = FontLoadingStrategy.Blocking,
)

@JvmInline
value class FontLoadingStrategy private constructor(val value: Int) {
    companion object {
        val Blocking = FontLoadingStrategy(0)
        val OptionalLocal = FontLoadingStrategy(1)
        val Async = FontLoadingStrategy(2)
    }
}

@JvmInline
value class FontSynthesis private constructor(val value: Int) {
    companion object {
        val None = FontSynthesis(0)
        val All = FontSynthesis(1)
        val Weight = FontSynthesis(2)
        val Style = FontSynthesis(3)
    }
}

class FontFamily private constructor(val name: String) {
    override fun equals(other: Any?): Boolean = other is FontFamily && name == other.name
    override fun hashCode(): Int = name.hashCode()

    sealed interface Resolver {
        suspend fun preload(fontFamily: FontFamily)
        fun resolve(
            fontFamily: FontFamily? = null,
            fontWeight: FontWeight = FontWeight.Normal,
            fontStyle: FontStyle = FontStyle.Normal,
            fontSynthesis: FontSynthesis = FontSynthesis.All,
        ): State<Any>
    }

    companion object {
        val Default = FontFamily("default")
        val SansSerif = FontFamily("sans-serif")
        val Serif = FontFamily("serif")
        val Monospace = FontFamily("monospace")
        val Cursive = FontFamily("cursive")

        operator fun invoke(vararg fonts: Font): FontFamily = invoke(fonts.toList())
        operator fun invoke(fonts: List<Font>): FontFamily {
            require(fonts.isNotEmpty()) { "A font family needs at least one font" }
            return FontFamily(Guest.fontFamily(fonts))
        }
    }
}

internal object HostFontFamilyResolver : FontFamily.Resolver {
    override suspend fun preload(fontFamily: FontFamily) {
        requestTextMeasurement(TextLayoutInput(
            AnnotatedString(""), TextStyle(fontFamily = fontFamily), Int.MAX_VALUE, true, TextOverflow.Clip,
        ))
    }

    override fun resolve(fontFamily: FontFamily?, fontWeight: FontWeight, fontStyle: FontStyle, fontSynthesis: FontSynthesis): State<Any> =
        mutableStateOf(HostTypeface(fontFamily ?: FontFamily.Default, fontWeight, fontStyle, fontSynthesis))
}

internal data class HostTypeface(
    val family: FontFamily, val weight: FontWeight, val style: FontStyle, val synthesis: FontSynthesis,
)
