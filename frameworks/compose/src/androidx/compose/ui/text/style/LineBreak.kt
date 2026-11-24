package androidx.compose.ui.text.style

import kotlin.jvm.JvmInline

data class LineBreak(val strategy: Strategy, val strictness: Strictness, val wordBreak: WordBreak) {
    @JvmInline
    value class Strategy private constructor(internal val wire: Int) {
        companion object {
            val Unspecified = Strategy(0)
            val Simple = Strategy(1)
            val HighQuality = Strategy(2)
            val Balanced = Strategy(3)
        }
    }

    @JvmInline
    value class Strictness private constructor(internal val wire: Int) {
        companion object {
            val Unspecified = Strictness(0)
            val Default = Strictness(1)
            val Loose = Strictness(2)
            val Normal = Strictness(3)
            val Strict = Strictness(4)
        }
    }

    @JvmInline
    value class WordBreak private constructor(internal val wire: Int) {
        companion object {
            val Unspecified = WordBreak(0)
            val Default = WordBreak(1)
            val Phrase = WordBreak(2)
        }
    }

    companion object {
        val Unspecified = LineBreak(Strategy.Unspecified, Strictness.Unspecified, WordBreak.Unspecified)
        val Simple = LineBreak(Strategy.Simple, Strictness.Normal, WordBreak.Default)
        val Heading = LineBreak(Strategy.Balanced, Strictness.Loose, WordBreak.Phrase)
        val Paragraph = LineBreak(Strategy.HighQuality, Strictness.Strict, WordBreak.Default)
    }
}

@JvmInline
value class Hyphens private constructor(internal val wire: Int) {
    companion object {
        val Unspecified = Hyphens(0)
        val None = Hyphens(1)
        val Auto = Hyphens(2)
    }
}
