package androidx.compose.ui.text

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.HostFontFamilyResolver
import androidx.compose.ui.text.style.ResolvedTextDirection
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.sp

data class TextRange(val start: Int, val end: Int) {
    constructor(index: Int) : this(index, index)
    val min: Int get() = minOf(start, end)
    val max: Int get() = maxOf(start, end)
    val collapsed: Boolean get() = start == end
    val reversed: Boolean get() = start > end
    val length: Int get() = max - min
    operator fun contains(offset: Int): Boolean = offset in min until max
    companion object { val Zero = TextRange(0) }
}

data class TextLayoutInput(
    val text: AnnotatedString,
    val style: TextStyle,
    val maxLines: Int,
    val softWrap: Boolean,
    val overflow: TextOverflow,
    val density: Density = object : Density { override val density = 1f },
    val layoutDirection: LayoutDirection = LayoutDirection.Ltr,
    val constraints: Constraints = Constraints(),
    val placeholders: List<AnnotatedString.Range<Placeholder>> = emptyList(),
    val fontFamilyResolver: FontFamily.Resolver = HostFontFamilyResolver,
)

internal data class TextGlyph(val start: Int, val end: Int, val left: Float, val right: Float, val rtl: Boolean)

internal data class TextLine(
    val start: Int, val end: Int, val visibleEnd: Int,
    val left: Float, val right: Float, val top: Float, val bottom: Float, val baseline: Float,
    val rtl: Boolean, val glyphs: List<TextGlyph>,
)

/** Measurements reported by the same host text layout used for rendering and hit testing. */
class TextLayoutResult internal constructor(
    val layoutInput: TextLayoutInput,
    val size: IntSize,
    private val lines: List<TextLine>,
    val didOverflowHeight: Boolean,
    val didOverflowWidth: Boolean,
    val placeholderRects: List<Rect?>,
    internal val didExceedMaxLines: Boolean,
    private val ellipsized: Boolean,
    private val wordOffsets: List<Int>,
) {
    val multiParagraph: MultiParagraph = MultiParagraph(this)
    val lineCount: Int get() = lines.size
    val firstBaseline: Float get() = lines.firstOrNull()?.baseline ?: 0f
    val lastBaseline: Float get() = lines.lastOrNull()?.baseline ?: 0f
    val hasVisualOverflow: Boolean get() = didOverflowHeight || didOverflowWidth

    fun getLineStart(lineIndex: Int): Int = lines[lineIndex].start
    fun getLineEnd(lineIndex: Int, visibleEnd: Boolean = false): Int =
        lines[lineIndex].let { if (visibleEnd) it.visibleEnd else it.end }
    fun getLineTop(lineIndex: Int): Float = lines[lineIndex].top
    fun getLineBottom(lineIndex: Int): Float = lines[lineIndex].bottom
    fun getLineLeft(lineIndex: Int): Float = lines[lineIndex].left
    fun getLineRight(lineIndex: Int): Float = lines[lineIndex].right
    fun getLineBaseline(lineIndex: Int): Float = lines[lineIndex].baseline
    fun isLineEllipsized(lineIndex: Int): Boolean =
        lineIndex == lines.lastIndex && ellipsized

    fun getLineForVerticalPosition(vertical: Float): Int =
        lines.indexOfFirst { vertical < it.bottom }.takeIf { it >= 0 } ?: lines.lastIndex.coerceAtLeast(0)

    fun getLineForOffset(offset: Int): Int =
        lines.indexOfLast { it.start <= offset }.coerceAtLeast(0)

    fun getParagraphDirection(offset: Int): ResolvedTextDirection =
        direction(lines.getOrNull(getLineForOffset(offset))?.rtl == true)

    fun getBidiRunDirection(offset: Int): ResolvedTextDirection {
        val line = lines.getOrNull(getLineForOffset(offset)) ?: return ResolvedTextDirection.Ltr
        return direction(line.glyphs.firstOrNull { offset in it.start until it.end }?.rtl ?: line.rtl)
    }

    fun getOffsetForPosition(position: Offset): Int {
        val line = lines.getOrNull(getLineForVerticalPosition(position.y)) ?: return 0
        if (line.glyphs.isEmpty()) return line.start
        val nearest = line.glyphs.minBy { g -> kotlin.math.abs(position.x - position.x.coerceIn(g.left, g.right)) }
        val rightHalf = position.x >= (nearest.left + nearest.right) / 2
        return if (rightHalf != nearest.rtl) nearest.end else nearest.start
    }

    fun getBoundingBox(offset: Int): Rect {
        require(offset in 0 until layoutInput.text.length)
        val line = lines.getOrNull(getLineForOffset(offset)) ?: return Rect.Zero
        val glyph = line.glyphs.firstOrNull { offset in it.start until it.end } ?: return getCursorRect(offset)
        return Rect(glyph.left, line.top, glyph.right, line.bottom)
    }

    fun getCursorRect(offset: Int): Rect {
        require(offset in 0..layoutInput.text.length)
        val line = lines.getOrNull(getLineForOffset(offset)) ?: return Rect.Zero
        val x = getHorizontalPosition(offset, usePrimaryDirection = true)
        return Rect(x, line.top, x, line.bottom)
    }

    fun getHorizontalPosition(offset: Int, usePrimaryDirection: Boolean): Float {
        val line = lines.getOrNull(getLineForOffset(offset)) ?: return 0f
        val following = line.glyphs.firstOrNull { offset in it.start until it.end }
        val preceding = line.glyphs.lastOrNull { offset == it.end }
        val preferredDirection = if (usePrimaryDirection) line.rtl else !line.rtl
        if (following != null && (preceding == null || following.rtl == preferredDirection)) {
            val fraction = (offset - following.start).toFloat() / (following.end - following.start)
            return if (following.rtl) following.right - fraction * (following.right - following.left)
                else following.left + fraction * (following.right - following.left)
        }
        return preceding?.let { if (it.rtl) it.left else it.right } ?: if (line.rtl) line.left else line.right
    }

    fun getWordBoundary(offset: Int): TextRange {
        val text = layoutInput.text.text
        if (text.isEmpty()) return TextRange.Zero
        var at = offset.coerceIn(0, text.lastIndex)
        if (wordOffsets.size >= 2) {
            val index = wordOffsets.binarySearch(at).let { if (it < 0) -it - 2 else it }.coerceIn(0, wordOffsets.lastIndex - 1)
            return TextRange(wordOffsets[index], wordOffsets[index + 1])
        }
        if (at > 0 && text[at].isLowSurrogate() && text[at - 1].isHighSurrogate()) at--
        val word = text[at].isLetterOrDigit()
        if (!word) {
            val count = if (text[at].isHighSurrogate() && text.getOrNull(at + 1)?.isLowSurrogate() == true) 2 else 1
            return TextRange(at, at + count)
        }
        var start = at
        var end = at + 1
        while (start > 0 && text[start - 1].isLetterOrDigit()) start--
        while (end < text.length && text[end].isLetterOrDigit()) end++
        return TextRange(start, end)
    }

    fun getBoundingBoxesForRange(start: Int, end: Int): List<Rect> {
        require(start in 0..end && end <= layoutInput.text.length)
        if (start == end) return emptyList()
        return lines.flatMap { line ->
            val boxes = mutableListOf<Rect>()
            for (glyph in line.glyphs.sortedBy { it.left }) {
                if (start >= glyph.end || glyph.start >= end) continue
                val last = boxes.lastOrNull()
                if (last != null && glyph.left <= last.right + 0.0001f) {
                    boxes[boxes.lastIndex] = last.copy(right = maxOf(last.right, glyph.right))
                } else {
                    boxes.add(Rect(glyph.left, line.top, glyph.right, line.bottom))
                }
            }
            boxes
        }
    }

    fun getPathForRange(start: Int, end: Int): Path = Path().apply {
        getBoundingBoxesForRange(start, end).forEach(::addRect)
    }

    fun getRangeForRect(rect: Rect, granularity: TextGranularity, inclusionStrategy: TextInclusionStrategy): TextRange {
        var start = Int.MAX_VALUE
        var end = 0
        for (line in lines) {
            val segments = mutableMapOf<TextRange, Rect>()
            for (glyph in line.glyphs) {
                if (glyph.start >= glyph.end || glyph.start >= layoutInput.text.length) continue
                val range = if (granularity == TextGranularity.Character) TextRange(glyph.start, glyph.end) else {
                    if (layoutInput.text[glyph.start].isWhitespace()) continue
                    getWordBoundary(glyph.start)
                }
                val previous = segments[range]
                segments[range] = Rect(
                    minOf(previous?.left ?: glyph.left, glyph.left), line.top,
                    maxOf(previous?.right ?: glyph.right, glyph.right), line.bottom,
                )
            }
            for ((range, bounds) in segments) {
                if (inclusionStrategy.isIncluded(bounds, rect)) {
                    start = minOf(start, range.min)
                    end = maxOf(end, range.max)
                }
            }
        }
        return if (end == 0) TextRange.Zero else TextRange(start, end)
    }

    fun copy(layoutInput: TextLayoutInput = this.layoutInput, size: IntSize = this.size): TextLayoutResult =
        TextLayoutResult(
            layoutInput, size, lines, didExceedMaxLines || (lines.lastOrNull()?.bottom ?: 0f) > size.height,
            lines.any { it.right - it.left > size.width }, placeholderRects, didExceedMaxLines, ellipsized, wordOffsets,
        )

    private fun direction(rtl: Boolean) = if (rtl) ResolvedTextDirection.Rtl else ResolvedTextDirection.Ltr

    internal companion object {
        fun decode(input: TextLayoutInput, geometry: String): TextLayoutResult {
            val records = geometry.split(';')
            val dimensions = records.first().split(',')
            val layoutRecords = records.drop(2).dropLast(1)
            val words = layoutRecords.lastOrNull()?.takeIf { it.startsWith('w') }
            val wordOffsets = words?.split(',')?.drop(1)?.map(String::toInt).orEmpty()
            val lines = (if (words == null) layoutRecords else layoutRecords.dropLast(1)).map { record ->
                val n = record.split(',')
                val glyphs = n.drop(9).chunked(5).map { g ->
                    TextGlyph(g[0].toInt(), g[1].toInt(), g[2].toFloat(), g[3].toFloat(), g[4] == "1")
                }
                TextLine(n[0].toInt(), n[1].toInt(), n[2].toInt(), n[3].toFloat(), n[4].toFloat(),
                    n[5].toFloat(), n[6].toFloat(), n[7].toFloat(), n[8] == "1", glyphs)
            }
            val size = IntSize(dimensions[0].toInt(), dimensions[1].toInt())
            val overflow = records.last().toInt()
            val placeholders = records[1].split(',').drop(1).chunked(4).map { values ->
                val rect = Rect(values[0].toFloat(), values[1].toFloat(), values[2].toFloat(), values[3].toFloat())
                rect.takeIf { it.width >= 0 && it.height >= 0 }
            }
            val density = Density(dimensions[2].toFloat(), input.density.fontScale)
            val constraints = if (dimensions.size >= 7) Constraints(
                dimensions[3].toInt(), dimensions[4].toInt(), dimensions[5].toInt(), dimensions[6].toInt(),
            ) else Constraints(maxWidth = size.width, maxHeight = size.height)
            val autoFontSize = dimensions.getOrNull(7)?.toFloatOrNull()?.takeIf { it >= 0f }
            val style = if (autoFontSize == null) input.style else input.style.copy(fontSize = autoFontSize.sp)
            return TextLayoutResult(
                input.copy(density = density, constraints = constraints, style = style), size, lines,
                overflow and 1 != 0, overflow and 2 != 0, placeholders, overflow and 4 != 0, overflow and 8 != 0, wordOffsets,
            )
        }
    }
}
