package androidx.compose.ui.text

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.drawscope.DrawStyle
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.BaselineShift
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextIndent
import androidx.compose.ui.text.style.Hyphens
import androidx.compose.ui.text.style.LineBreak
import androidx.compose.ui.text.intl.LocaleList
import androidx.compose.ui.text.style.TextGeometricTransform
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp

@Immutable
data class SpanStyle(
    val color: Color = Color.Unspecified,
    val fontSize: TextUnit = TextUnit.Unspecified,
    val fontWeight: FontWeight? = null,
    val fontStyle: FontStyle? = null,
    val textDecoration: TextDecoration? = null,
    val background: Color = Color.Unspecified,
    val letterSpacing: TextUnit = TextUnit.Unspecified,
    val fontFamily: FontFamily? = null,
    val baselineShift: BaselineShift? = null,
    val fontFeatureSettings: String? = null,
    val brush: Brush? = null,
    val alpha: Float = Float.NaN,
    val shadow: Shadow? = null,
    val drawStyle: DrawStyle? = null,
    val textGeometricTransform: TextGeometricTransform? = null,
    val localeList: LocaleList? = null,
) : AnnotatedString.Annotation

@Immutable
data class ParagraphStyle(
    val textAlign: TextAlign = TextAlign.Unspecified,
    val lineHeight: TextUnit = TextUnit.Unspecified,
    val textIndent: TextIndent? = null,
    val lineBreak: LineBreak = LineBreak.Unspecified,
    val hyphens: Hyphens = Hyphens.Unspecified,
) : AnnotatedString.Annotation

/** Ranges use the same UTF-16 indices as Kotlin strings. */
@Immutable
class AnnotatedString(
    val text: String,
    annotations: List<Range<out Annotation>>,
) : CharSequence {
    interface Annotation

    data class Range<T>(val item: T, val start: Int, val end: Int, val tag: String = "") {
        init { require(start <= end) { "Reversed range: $start..$end" } }
    }

    internal val annotations = annotations.toList()
    val spanStyles: List<Range<SpanStyle>>
        get() = annotations.mapNotNull { r -> (r.item as? SpanStyle)?.let { Range(it, r.start, r.end, r.tag) } }
    val paragraphStyles: List<Range<ParagraphStyle>>
        get() = annotations.mapNotNull { r -> (r.item as? ParagraphStyle)?.let { Range(it, r.start, r.end, r.tag) } }

    internal val bullets: List<Range<Bullet>>
        get() = annotations.mapNotNull { range ->
            (range.item as? Bullet)?.let { Range(it, range.start, range.end, range.tag) }
        }

    init {
        val ends = mutableListOf<Int>()
        for (range in paragraphStyles.sortedWith(compareBy({ it.start }, { -it.end }))) {
            while (ends.lastOrNull()?.let { it <= range.start } == true) ends.removeAt(ends.lastIndex)
            require(ends.lastOrNull()?.let { range.end <= it } != false) { "Paragraph ranges must be nested or disjoint" }
            ends.add(range.end)
        }
    }

    constructor(text: String, spanStyles: List<Range<SpanStyle>> = emptyList(), paragraphStyles: List<Range<ParagraphStyle>> = emptyList()) :
        this(text, annotations = spanStyles + paragraphStyles)

    override val length: Int get() = text.length
    override fun get(index: Int): Char = text[index]
    override fun subSequence(startIndex: Int, endIndex: Int): AnnotatedString {
        val content = text.substring(startIndex, endIndex)
        if (startIndex == 0 && endIndex == length) return this
        val ranges = annotations.filter { intersects(startIndex, endIndex, it.start, it.end) }.map {
            Range(it.item, maxOf(startIndex, it.start) - startIndex, minOf(endIndex, it.end) - startIndex, it.tag)
        }
        return AnnotatedString(content, annotations = ranges)
    }

    fun subSequence(range: TextRange): AnnotatedString = subSequence(range.min, range.max)

    fun hasEqualAnnotations(other: AnnotatedString): Boolean = annotations == other.annotations

    fun getStringAnnotations(tag: String, start: Int, end: Int): List<Range<String>> =
        getStringAnnotations(start, end).filter { it.tag == tag }

    fun getStringAnnotations(start: Int, end: Int): List<Range<String>> =
        annotations.mapNotNull {
            val item = it.item as? StringAnnotation
            if (item != null && intersects(start, end, it.start, it.end)) Range(item.value, it.start, it.end, it.tag) else null
        }

    fun hasStringAnnotations(tag: String, start: Int, end: Int): Boolean =
        annotations.any { it.item is StringAnnotation && it.tag == tag && intersects(start, end, it.start, it.end) }

    fun getLinkAnnotations(start: Int, end: Int): List<Range<LinkAnnotation>> = annotationsOfType(start, end)

    fun getTtsAnnotations(start: Int, end: Int): List<Range<TtsAnnotation>> = annotationsOfType(start, end)

    @Suppress("DEPRECATION")
    fun getUrlAnnotations(start: Int, end: Int): List<Range<UrlAnnotation>> = annotationsOfType(start, end)

    fun hasLinkAnnotations(start: Int, end: Int): Boolean = getLinkAnnotations(start, end).isNotEmpty()

    private inline fun <reified T : Annotation> annotationsOfType(start: Int, end: Int): List<Range<T>> =
        annotations.mapNotNull { range ->
            val item = range.item as? T
            if (item != null && intersects(start, end, range.start, range.end)) {
                Range(item, range.start, range.end, range.tag)
            } else null
        }

    fun mapAnnotations(transform: (Range<out Annotation>) -> Range<out Annotation>): AnnotatedString =
        AnnotatedString(text, annotations = annotations.map(transform))

    fun flatMapAnnotations(transform: (Range<out Annotation>) -> List<Range<out Annotation>>): AnnotatedString =
        AnnotatedString(text, annotations = annotations.flatMap(transform))

    operator fun plus(other: AnnotatedString): AnnotatedString = buildAnnotatedString {
        append(this@AnnotatedString)
        append(other)
    }

    override fun toString(): String = text
    override fun equals(other: Any?): Boolean = other is AnnotatedString && text == other.text && annotations == other.annotations
    override fun hashCode(): Int = 31 * text.hashCode() + annotations.hashCode()

    class Builder(capacity: Int = 16) : Appendable {
        private val content = StringBuilder(capacity)
        private data class MutableRange(val item: Annotation, val start: Int, var end: Int? = null, val tag: String = "")
        private val ranges = mutableListOf<MutableRange>()
        private val stack = mutableListOf<MutableRange>()

        val length: Int get() = content.length

        constructor(text: String) : this() { append(text) }
        constructor(text: AnnotatedString) : this() { append(text) }

        override fun append(value: CharSequence?): Builder = apply {
            if (value is AnnotatedString) append(value) else content.append(value)
        }

        override fun append(value: CharSequence?, startIndex: Int, endIndex: Int): Builder = apply {
            if (value is AnnotatedString) append(value.subSequence(startIndex, endIndex))
            else content.append(value, startIndex, endIndex)
        }

        override fun append(value: Char): Builder = apply { content.append(value) }

        fun append(text: String) {
            content.append(text)
        }

        fun append(text: AnnotatedString) {
            val offset = length
            content.append(text.text)
            text.annotations.forEach { ranges.add(MutableRange(it.item, offset + it.start, offset + it.end, it.tag)) }
        }

        fun append(text: AnnotatedString, start: Int, end: Int) = append(text.subSequence(start, end))

        fun addStyle(style: SpanStyle, start: Int, end: Int) {
            ranges.add(MutableRange(style, start, end))
        }

        fun addStyle(style: ParagraphStyle, start: Int, end: Int) {
            ranges.add(MutableRange(style, start, end))
        }

        fun addStringAnnotation(tag: String, annotation: String, start: Int, end: Int) {
            ranges.add(MutableRange(StringAnnotation(annotation), start, end, tag))
        }

        fun addLink(url: LinkAnnotation.Url, start: Int, end: Int) {
            ranges.add(MutableRange(url, start, end))
        }

        fun addLink(clickable: LinkAnnotation.Clickable, start: Int, end: Int) {
            ranges.add(MutableRange(clickable, start, end))
        }

        fun addTtsAnnotation(ttsAnnotation: TtsAnnotation, start: Int, end: Int) {
            ranges.add(MutableRange(ttsAnnotation, start, end))
        }

        @Suppress("DEPRECATION")
        fun addUrlAnnotation(urlAnnotation: UrlAnnotation, start: Int, end: Int) {
            ranges.add(MutableRange(urlAnnotation, start, end))
        }

        fun addBullet(bullet: Bullet, start: Int, end: Int) {
            ranges.add(MutableRange(bullet, start, end))
        }

        fun addBullet(bullet: Bullet, indentation: TextUnit, start: Int, end: Int) {
            addStyle(ParagraphStyle(textIndent = TextIndent(indentation, indentation)), start, end)
            addBullet(bullet, start, end)
        }

        fun pushBullet(bullet: Bullet): Int = push(MutableRange(bullet, length))

        class BulletScope internal constructor() {
            internal val settings = mutableListOf<Pair<TextUnit, Bullet>>()
        }

        private val bulletScope = BulletScope()

        fun <R : Any> withBulletList(
            indentation: TextUnit = Bullet.DefaultIndentation,
            bullet: Bullet = Bullet.Default,
            block: BulletScope.() -> R,
        ): R {
            val inherited = bulletScope.settings.lastOrNull()?.first
            val adjusted = if (inherited == null) indentation else {
                require(inherited.isSp == indentation.isSp && inherited.isEm == indentation.isEm) {
                    "Indentation unit types of nested bullet lists must match"
                }
                val sum = inherited.value + indentation.value
                when {
                    indentation.isEm -> sum.em
                    indentation.isSp -> sum.sp
                    else -> indentation
                }
            }
            val index = pushStyle(ParagraphStyle(textIndent = TextIndent(adjusted, adjusted)))
            bulletScope.settings.add(adjusted to bullet)
            return try {
                bulletScope.block()
            } finally {
                bulletScope.settings.removeAt(bulletScope.settings.lastIndex)
                pop(index)
            }
        }

        fun <R : Any> BulletScope.withBulletListItem(bullet: Bullet? = null, block: Builder.() -> R): R {
            val (indent, defaultBullet) = settings.last()
            val index = pushStyle(ParagraphStyle(textIndent = TextIndent(indent, indent)))
            pushBullet(bullet ?: defaultBullet)
            return try { block() } finally { pop(index) }
        }

        fun pushStyle(style: SpanStyle): Int = push(MutableRange(style, length))
        fun pushStyle(style: ParagraphStyle): Int = push(MutableRange(style, length))

        fun pushStringAnnotation(tag: String, annotation: String): Int =
            push(MutableRange(StringAnnotation(annotation), length, tag = tag))

        fun pushLink(link: LinkAnnotation): Int = push(MutableRange(link, length))
        fun pushTtsAnnotation(ttsAnnotation: TtsAnnotation): Int = push(MutableRange(ttsAnnotation, length))
        @Suppress("DEPRECATION")
        fun pushUrlAnnotation(urlAnnotation: UrlAnnotation): Int = push(MutableRange(urlAnnotation, length))

        private fun push(range: MutableRange): Int {
            ranges.add(range)
            stack.add(range)
            return stack.lastIndex
        }

        fun pop() {
            check(stack.isNotEmpty()) { "Nothing to pop" }
            stack.removeAt(stack.lastIndex).end = length
        }

        fun pop(index: Int) {
            check(index in stack.indices) { "No style or annotation at index $index" }
            while (stack.size > index) pop()
        }

        fun toAnnotatedString(): AnnotatedString =
            AnnotatedString(content.toString(), annotations = ranges.map { Range(it.item, it.start, it.end ?: length, it.tag) })
    }
}

fun AnnotatedString(text: String, spanStyle: SpanStyle?, paragraphStyle: ParagraphStyle? = null): AnnotatedString =
    AnnotatedString(text, listOfNotNull(spanStyle?.let { AnnotatedString.Range(it, 0, text.length) }),
        listOfNotNull(paragraphStyle?.let { AnnotatedString.Range(it, 0, text.length) }))

fun AnnotatedString(text: String, paragraphStyle: ParagraphStyle): AnnotatedString =
    AnnotatedString(text, spanStyle = null, paragraphStyle = paragraphStyle)

private data class StringAnnotation(val value: String) : AnnotatedString.Annotation

private fun intersects(start: Int, end: Int, otherStart: Int, otherEnd: Int): Boolean =
    (start < otherEnd && otherStart < end) || (start == otherStart && (start == end || otherStart == otherEnd))

inline fun buildAnnotatedString(builder: AnnotatedString.Builder.() -> Unit): AnnotatedString =
    AnnotatedString.Builder().apply(builder).toAnnotatedString()

inline fun <R> AnnotatedString.Builder.withStyle(style: SpanStyle, block: AnnotatedString.Builder.() -> R): R {
    val index = pushStyle(style)
    return try { block() } finally { pop(index) }
}

inline fun <R> AnnotatedString.Builder.withStyle(style: ParagraphStyle, block: AnnotatedString.Builder.() -> R): R {
    val index = pushStyle(style)
    return try { block() } finally { pop(index) }
}

inline fun <R> AnnotatedString.Builder.withAnnotation(
    tag: String,
    annotation: String,
    block: AnnotatedString.Builder.() -> R,
): R {
    val index = pushStringAnnotation(tag, annotation)
    return try { block() } finally { pop(index) }
}

inline fun <R> AnnotatedString.Builder.withLink(link: LinkAnnotation, block: AnnotatedString.Builder.() -> R): R {
    val index = pushLink(link)
    return try { block() } finally { pop(index) }
}

inline fun <R> AnnotatedString.Builder.withAnnotation(
    ttsAnnotation: TtsAnnotation,
    block: AnnotatedString.Builder.() -> R,
): R {
    val index = pushTtsAnnotation(ttsAnnotation)
    return try { block() } finally { pop(index) }
}

@Suppress("DEPRECATION")
inline fun <R> AnnotatedString.Builder.withAnnotation(
    urlAnnotation: UrlAnnotation,
    block: AnnotatedString.Builder.() -> R,
): R {
    val index = pushUrlAnnotation(urlAnnotation)
    return try { block() } finally { pop(index) }
}
