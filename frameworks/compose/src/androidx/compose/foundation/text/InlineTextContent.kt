package androidx.compose.foundation.text

import androidx.compose.runtime.Composable
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.Placeholder

private const val InlineContentTag = "androidx.compose.foundation.text.inlineContent"

class InlineTextContent(val placeholder: Placeholder, val children: @Composable (String) -> Unit)

fun AnnotatedString.Builder.appendInlineContent(id: String, alternateText: String = "\uFFFD") {
    require(alternateText.isNotEmpty())
    val start = length
    append(alternateText)
    addStringAnnotation(InlineContentTag, id, start, length)
}

internal data class TextSlot<T>(
    val originalStart: Int,
    val originalEnd: Int,
    val offset: Int,
    val alternateText: String,
    val content: T,
)

internal fun resolveInlineContent(
    text: AnnotatedString,
    inlineContent: Map<String, InlineTextContent>,
): Pair<AnnotatedString, List<TextSlot<InlineTextContent>>> {
    if (inlineContent.isEmpty()) return text to emptyList()
    val ranges = text.getStringAnnotations(InlineContentTag, 0, text.length).mapNotNull { range ->
        val content = inlineContent[range.item] ?: return@mapNotNull null
        AnnotatedString.Range(content, range.start, range.end)
    }
    return resolveTextSlots(text, ranges)
}

internal fun <T> resolveTextSlots(
    text: AnnotatedString,
    ranges: List<AnnotatedString.Range<T>>,
): Pair<AnnotatedString, List<TextSlot<T>>> {
    if (ranges.isEmpty()) return text to emptyList()
    var previousEnd = 0
    val display = StringBuilder()
    val slots = ranges
        .sortedBy { it.start }
        .mapNotNull { range ->
            if (range.start < previousEnd || range.start >= range.end || range.end > text.length) return@mapNotNull null
            display.append(text.text, previousEnd, range.start)
            val slot = TextSlot(range.start, range.end, display.length, text.text.substring(range.start, range.end), range.item)
            display.append('\uFFFC')
            previousEnd = range.end
            slot
        }
    if (slots.isEmpty()) return text to emptyList()
    display.append(text.text, previousEnd, text.length)
    fun index(offset: Int, isEnd: Boolean): Int {
        var removed = 0
        for (slot in slots) {
            if (offset <= slot.originalStart) break
            if (offset < slot.originalEnd) return slot.offset + if (isEnd) 1 else 0
            removed += slot.originalEnd - slot.originalStart - 1
        }
        return offset - removed
    }
    return AnnotatedString(display.toString(), annotations = text.annotations.map { range ->
        AnnotatedString.Range(range.item, index(range.start, false), index(range.end, true), range.tag)
    }) to slots
}
