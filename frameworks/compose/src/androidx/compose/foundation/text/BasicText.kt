package androidx.compose.foundation.text

import androidx.compose.foundation.text.modifiers.MeasuredTextAutoSizeScope
import androidx.compose.foundation.text.modifiers.TextProbe
import androidx.compose.foundation.text.modifiers.TextProbeRequired
import androidx.compose.foundation.text.selection.LocalTextSelectable
import androidx.compose.foundation.text.selection.LocalTextSelectionColors
import androidx.compose.foundation.wire
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.needsSize
import androidx.compose.ui.graphics.wire
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.DrawStyle
import androidx.compose.ui.graphics.drawscope.Fill
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.wire
import androidx.compose.ui.jsonNum
import androidx.compose.ui.jsonString
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.ParagraphStyle
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutInput
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.TtsAnnotation
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDirection
import androidx.compose.ui.text.style.Hyphens
import androidx.compose.ui.text.style.LineBreak
import androidx.compose.ui.text.intl.Locale
import androidx.compose.ui.text.style.TextGeometricTransform
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.text.style.ResolvedTextDirection
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.sp
import onesim.compose.Primitive
import onesim.compose.TextPrimitive

/** Text primitive (protocol section K). Font size defaults to 14 sp, color to black, as in Compose. */
@Composable
fun BasicText(
    text: String,
    modifier: Modifier = Modifier,
    style: TextStyle = TextStyle.Default,
    onTextLayout: ((TextLayoutResult) -> Unit)? = null,
    overflow: TextOverflow = TextOverflow.Clip,
    softWrap: Boolean = true,
    maxLines: Int = Int.MAX_VALUE,
    minLines: Int = 1,
    color: (() -> Color)? = null,
    autoSize: TextAutoSize? = null,
) {
    TextContent(text, null, modifier, style, overflow, softWrap, maxLines, minLines, color, onTextLayout, autoSize = autoSize)
}

@Composable
fun BasicText(
    text: AnnotatedString,
    modifier: Modifier = Modifier,
    style: TextStyle = TextStyle.Default,
    onTextLayout: ((TextLayoutResult) -> Unit)? = null,
    overflow: TextOverflow = TextOverflow.Clip,
    softWrap: Boolean = true,
    maxLines: Int = Int.MAX_VALUE,
    minLines: Int = 1,
    inlineContent: Map<String, InlineTextContent> = emptyMap(),
    color: (() -> Color)? = null,
    autoSize: TextAutoSize? = null,
) {
    TextContent(text.text, text, modifier, style, overflow, softWrap, maxLines, minLines, color, onTextLayout, inlineContent = inlineContent, autoSize = autoSize)
}

@Deprecated("Use BasicText or Text with LinkAnnotation in Jetpack Compose.")
@Composable
fun ClickableText(
    text: AnnotatedString,
    modifier: Modifier = Modifier,
    style: TextStyle = TextStyle.Default,
    softWrap: Boolean = true,
    overflow: TextOverflow = TextOverflow.Clip,
    maxLines: Int = Int.MAX_VALUE,
    onTextLayout: (TextLayoutResult) -> Unit = {},
    onClick: (Int) -> Unit,
) {
    TextContent(text.text, text, modifier, style, overflow, softWrap, maxLines, 1, null, onTextLayout, onClick)
}

@Composable
private fun TextContent(
    text: String,
    original: AnnotatedString?,
    modifier: Modifier,
    baseStyle: TextStyle,
    overflow: TextOverflow,
    softWrap: Boolean,
    maxLines: Int,
    minLines: Int,
    color: (() -> Color)?,
    onTextLayout: ((TextLayoutResult) -> Unit)?,
    onClick: ((Int) -> Unit)? = null,
    inlineContent: Map<String, InlineTextContent> = emptyMap(),
    autoSize: TextAutoSize? = null,
    probeConstraints: Constraints? = null,
) {
    require(minLines in 1..maxLines) { "Expected 1 <= minLines <= maxLines" }
    val resolved = original?.let { resolveInlineContent(it, inlineContent) }
    val annotated = resolved?.first
    val slots = resolved?.second.orEmpty()
    val displayText = annotated?.text ?: text
    var style = paragraphDefaults(baseStyle, annotated, displayText)
    val chosenColor = color?.invoke()
    val custom = autoSize?.takeUnless { it is StepTextAutoSize }
    val customState = if (custom == null) null else remember(custom, original, text, style, overflow, softWrap, maxLines, inlineContent, chosenColor) {
        CustomAutoSizeState()
    }
    val baseSize = style.fontSize.takeIf { it.isSp }?.value ?: 14f
    val textInput = original ?: AnnotatedString(text)
    var pending: TextProbe? = null
    val environment = customState?.environment
    if (custom != null && environment != null) {
        val (constraints, density) = environment
        val scope = MeasuredTextAutoSizeScope(customState.measured, density, baseSize)
        try {
            val selected = with(custom) { scope.getFontSize(constraints, textInput) }
            val size = selected.spAt(baseSize)
            require(selected.isSpecified && size.isFinite() && size >= 0f) { "Expected a nonnegative font size" }
            style = style.copy(fontSize = size.sp)
        } catch (request: TextProbeRequired) {
            pending = request.probe
        }
    }
    val c = chosenColor?.takeIf { it.isSpecified } ?: style.color.takeIf { it.isSpecified } ?: Color.Black
    val selectionColors = if (LocalTextSelectable.current) LocalTextSelectionColors.current else null
    val links = annotated?.getLinkAnnotations(0, displayText.length).orEmpty().filter { it.start < it.end }
    val needsBrushSize = style.brush?.needsSize == true ||
        annotated?.spanStyles.orEmpty().any { it.item.brush?.needsSize == true } ||
        annotated?.bullets.orEmpty().any {
            it.item.brush?.needsSize == true || (it.item.shape !== RectangleShape && it.item.shape.wire() == null)
        } ||
        links.any { link -> link.item.styles?.let {
            listOf(it.style, it.focusedStyle, it.hoveredStyle, it.pressedStyle).any { span -> span?.brush?.needsSize == true }
        } == true }
    var shaderLayout by remember { mutableStateOf<ShaderLayout?>(null) }
    val shaderSize = shaderLayout?.size
    val uriHandler = LocalUriHandler.current
    val layoutDirection = LocalLayoutDirection.current
    val size = if (style.fontSize.isSpecified) style.fontSize.value else 14f
    val p = buildString {
        append("\"text\":").append(jsonString(displayText))
        probeConstraints?.let {
            append(",\"probeConstraints\":[").append(it.minWidth).append(',').append(it.maxWidth)
                .append(',').append(it.minHeight).append(',').append(it.maxHeight).append(']')
        }
        if (customState != null) {
            val awaiting = environment == null || pending != null
            append(",\"customAutoSize\":").append(awaiting)
        }
        if (autoSize is StepTextAutoSize) {
            append(",\"autoSize\":[").append(autoSize.minimum.wireUnit()).append(',')
                .append(autoSize.maximum.wireUnit()).append(',').append(autoSize.step.wireUnit()).append(']')
        }
        appendInlineObjects(text, slots, size, InlineTextContent::placeholder)
        appendTextStyle(style, c, chosenColor, annotated, shaderSize, maxLines, minLines, softWrap, overflow, layoutDirection)
        selectionColors?.let {
            append(",\"selectionColor\":").append(it.backgroundColor.wire())
            append(",\"selectionHandleColor\":").append(it.handleColor.wire())
        }
        if (onClick == null && links.isNotEmpty()) {
            append(",\"linkRanges\":[")
            links.forEachIndexed { i, link ->
                if (i > 0) append(',')
                append('[').append(link.start).append(',').append(link.end).append(']')
            }
            append(']')
            if (links.any { it.item.styles?.let { styles ->
                styles.focusedStyle != null || styles.hoveredStyle != null || styles.pressedStyle != null
            } == true }) {
                append(",\"linkStyles\":[")
                links.forEachIndexed { i, link ->
                    if (i > 0) append(',')
                    val styles = link.item.styles
                    append('[')
                    listOf(styles?.focusedStyle, styles?.hoveredStyle, styles?.pressedStyle).forEachIndexed { index, span ->
                        if (index > 0) append(',')
                        if (span == null) append("null") else appendSpan(span, link.start, link.end, shaderSize)
                    }
                    append(']')
                }
                append(']')
            }
        }
        val speech = annotated?.getTtsAnnotations(0, displayText.length).orEmpty()
        if (speech.isNotEmpty()) {
            append(",\"speechRanges\":[")
            speech.forEachIndexed { i, range ->
                if (i > 0) append(',')
                val spoken = (range.item as TtsAnnotation.Verbatim).verbatim
                append('[').append(range.start).append(',').append(range.end).append(',').append(jsonString(spoken)).append(']')
            }
            append(']')
        }
        appendAnnotatedStyles(annotated, style, displayText, links, shaderLayout, layoutDirection)
    }
    val onLayout: ((String) -> Unit)? = if (onTextLayout == null && customState == null && !needsBrushSize) null else {
        val input = TextLayoutInput(textInput, style, maxLines, softWrap, overflow, layoutDirection = layoutDirection,
            placeholders = slots.map { AnnotatedString.Range(it.content.placeholder, it.originalStart, it.originalEnd) })
        val deliver: (String) -> Unit = { data ->
            val result = TextLayoutResult.decode(input, data)
            if (needsBrushSize) {
                shaderLayout = ShaderLayout(
                    Size(result.size.width.toFloat(), result.size.height.toFloat()),
                    result.layoutInput.density.density,
                    result.layoutInput.style.fontSize.takeIf { it.isSpecified }?.value ?: size,
                    annotated?.bullets.orEmpty().associate { range ->
                        val rtl = result.getParagraphDirection(range.start) == ResolvedTextDirection.Rtl
                        range.start to if (rtl) LayoutDirection.Rtl else LayoutDirection.Ltr
                    },
                )
            }
            if (customState != null) {
                val next = result.layoutInput.constraints to result.layoutInput.density
                val previous = customState.environment
                if (previous == null || previous.first != next.first || previous.second.density != next.second.density) {
                    customState.measured.clear()
                    customState.environment = next
                } else if (pending == null) {
                    onTextLayout?.invoke(result)
                }
            } else onTextLayout?.invoke(result)
        }
        deliver
    }
    val activate = onClick ?: if (links.isNotEmpty()) { offset: Int ->
        links.lastOrNull { offset in it.start until it.end }?.item?.let { link ->
            val listener = link.linkInteractionListener
            if (listener != null) listener.onClick(link)
            else if (link is LinkAnnotation.Url) uriHandler.openUri(link.url)
        }
        Unit
    } else null
    TextPrimitive(modifier, p, activate.takeUnless { probeConstraints != null }, onLayout, isProbe = probeConstraints != null) {
        if (probeConstraints == null) {
            for (slot in slots) Primitive("Box", Modifier, "") {
                slot.content.children(slot.alternateText)
            }
        }
        pending?.let { request ->
            TextContent(
                request.text.text, request.text, Modifier, baseStyle.copy(fontSize = request.fontSize),
                overflow, softWrap, maxLines, minLines, color,
                onTextLayout = { result -> customState?.measured?.set(request, result) },
                inlineContent = inlineContent,
                probeConstraints = request.constraints,
            )
        }
    }
}

internal fun <T> StringBuilder.appendInlineObjects(
    text: String,
    slots: List<TextSlot<T>>,
    size: Float,
    placeholder: (T) -> Placeholder,
) {
    if (slots.isNotEmpty()) {
        append(",\"sourceText\":").append(jsonString(text))
        append(",\"inlineObjects\":[")
        slots.forEachIndexed { i, slot ->
            if (i > 0) append(',')
            val placeholder = placeholder(slot.content)
            append('[').append(slot.offset).append(',').append(slot.originalStart).append(',').append(slot.originalEnd)
            append(',').append(jsonNum(placeholder.width.spAt(size))).append(',').append(jsonNum(placeholder.height.spAt(size)))
            append(',').append(jsonString(placeholder.placeholderVerticalAlign.wireValue))
            if (placeholder.width.isEm || placeholder.height.isEm) {
                append(',').append(placeholder.width.isEm).append(',').append(placeholder.height.isEm)
            }
            append(']')
        }
        append(']')
    }
}

internal fun paragraphDefaults(baseStyle: TextStyle, annotated: AnnotatedString?, displayText: String): TextStyle {
    return annotated?.paragraphStyles.orEmpty().fold(baseStyle) { current, range ->
        if (range.start == 0 && range.end == displayText.length) {
            current.merge(TextStyle(
                textAlign = range.item.textAlign, lineHeight = range.item.lineHeight,
                lineBreak = range.item.lineBreak, hyphens = range.item.hyphens,
                textDirection = range.item.textDirection,
            ))
        } else current
    }
}

internal fun StringBuilder.appendTextStyle(
    style: TextStyle,
    c: Color,
    chosenColor: Color?,
    annotated: AnnotatedString?,
    shaderSize: Size?,
    maxLines: Int,
    minLines: Int,
    softWrap: Boolean,
    overflow: TextOverflow,
    layoutDirection: LayoutDirection,
) {
    val size = if (style.fontSize.isSpecified) style.fontSize.value else 14f
    append(",\"color\":").append(c.wire())
    if (chosenColor?.isSpecified != true) style.brush?.wire(shaderSize)?.let {
        append(",\"brush\":").append(it)
    }
    if (style.alpha.isFinite()) append(",\"paintAlpha\":").append(jsonNum(style.alpha))
    style.shadow?.let { append(",\"textShadow\":").append(it.wire()) }
    appendInkStyle(style.drawStyle)
    style.textGeometricTransform?.let { append(",\"textTransform\":").append(it.wire()) }
    append(",\"fontSize\":").append(jsonNum(size))
    appendParagraphBreak(style.lineBreak, style.hyphens)
    if (style.localeList != null || style.hyphens == Hyphens.Auto ||
        annotated?.paragraphStyles.orEmpty().any { it.item.hyphens == Hyphens.Auto }) {
        append(",\"textLocale\":").append(jsonString((style.localeList?.firstOrNull() ?: Locale.current).toLanguageTag()))
    }
    if (style.lineHeight.isSpecified) {
        append(if (style.lineHeight.isEm) ",\"lineHeightEm\":" else ",\"lineHeight\":").append(jsonNum(style.lineHeight.value))
    }
    append(",\"fontWeight\":").append((style.fontWeight ?: FontWeight.Normal).weight)
    if (style.letterSpacing.isSpecified) {
        append(if (style.letterSpacing.isEm) ",\"letterSpacingEm\":" else ",\"letterSpacing\":").append(jsonNum(style.letterSpacing.value))
    }
    when (style.textAlign) {
        TextAlign.Center -> append(",\"textAlign\":\"center\"")
        TextAlign.Justify -> append(",\"textAlign\":\"justify\"")
        TextAlign.End -> append(",\"textAlign\":\"end\"")
        TextAlign.Right -> append(",\"textAlign\":\"right\"")
        TextAlign.Left -> append(",\"textAlign\":\"left\"")
    }
    val direction = style.textDirection.resolve(layoutDirection)
    if (direction != TextDirection.ContentOrLtr) append(",\"textDirection\":").append(direction.value)
    if (maxLines != Int.MAX_VALUE) append(",\"maxLines\":").append(maxLines)
    if (minLines != 1) append(",\"minLines\":").append(minLines)
    if (!softWrap) append(",\"softWrap\":false")
    when (overflow) {
        TextOverflow.Visible -> append(",\"textOverflow\":\"visible\"")
        TextOverflow.Ellipsis -> append(",\"textOverflow\":\"ellipsis\"")
        TextOverflow.StartEllipsis -> append(",\"textOverflow\":\"startEllipsis\"")
        TextOverflow.MiddleEllipsis -> append(",\"textOverflow\":\"middleEllipsis\"")
    }
    if (style.fontStyle == FontStyle.Italic) append(",\"italic\":true")
    style.fontFeatureSettings?.let { append(",\"fontFeatureSettings\":").append(jsonString(it)) }
    style.fontFamily?.let { append(",\"fontFamily\":").append(jsonString(it.name)) }
    style.textDecoration?.let { append(",\"decoration\":").append(it.mask) }
}

internal fun StringBuilder.appendAnnotatedStyles(
    annotated: AnnotatedString?,
    style: TextStyle,
    displayText: String,
    links: List<AnnotatedString.Range<LinkAnnotation>>,
    shaderLayout: ShaderLayout? = null,
    layoutDirection: LayoutDirection,
) {
    val shaderSize = shaderLayout?.size
    val spans = annotated?.spanStyles.orEmpty() + links.mapNotNull { link ->
        link.item.styles?.style?.let { AnnotatedString.Range(it, link.start, link.end) }
    }
    val paragraphs = listOfNotNull(style.textIndent?.let {
        AnnotatedString.Range(ParagraphStyle(textIndent = it), 0, displayText.length)
    }) + annotated?.paragraphStyles.orEmpty()
    val bullets = annotated?.bullets.orEmpty().filter {
        it.item.width.isSpecified && it.item.height.isSpecified && it.item.padding.isSpecified
    }
    if (spans.isNotEmpty() || paragraphs.isNotEmpty() || bullets.isNotEmpty()) {
        append(",\"spans\":[")
        spans.forEachIndexed { i, range ->
            if (i > 0) append(',')
            appendSpan(range.item, range.start, range.end, shaderSize)
        }
        paragraphs.forEachIndexed { i, range ->
            if (spans.isNotEmpty() || i > 0) append(',')
            append("{\"start\":").append(range.start).append(",\"end\":").append(range.end)
            append(",\"paragraph\":").append(i + 1)
            range.item.textDirection.takeUnless { it == TextDirection.Unspecified }?.let {
                append(",\"paragraphDirection\":").append(it.resolve(layoutDirection).value)
            }
            appendParagraphBreak(range.item.lineBreak, range.item.hyphens)
            range.item.textIndent?.takeIf { it.firstLine.isSpecified && it.restLine.isSpecified }?.let {
                append(",\"paragraphIndent\":[").append(it.firstLine.wireUnit()).append(',').append(it.restLine.wireUnit()).append(']')
            }
            if (range.item.lineHeight.isSpecified) {
                append(if (range.item.lineHeight.isEm) ",\"paragraphHeightEm\":" else ",\"paragraphHeight\":")
                    .append(jsonNum(range.item.lineHeight.value))
            }
            when (range.item.textAlign) {
                TextAlign.Center -> append(",\"paragraphAlign\":\"center\"")
                TextAlign.Justify -> append(",\"paragraphAlign\":\"justify\"")
                TextAlign.End -> append(",\"paragraphAlign\":\"end\"")
                TextAlign.Start -> append(",\"paragraphAlign\":\"start\"")
                TextAlign.Left -> append(",\"paragraphAlign\":\"left\"")
                TextAlign.Right -> append(",\"paragraphAlign\":\"right\"")
            }
            append('}')
        }
        bullets.forEachIndexed { i, range ->
            if (spans.isNotEmpty() || paragraphs.isNotEmpty() || i > 0) append(',')
            val bullet = range.item
            append("{\"start\":").append(range.start).append(",\"end\":").append(range.end)
            append(",\"bullet\":{\"width\":").append(bullet.width.wireUnit())
            append(",\"height\":").append(bullet.height.wireUnit())
            append(",\"padding\":").append(bullet.padding.wireUnit())
            val layout = shaderLayout
            val markerSize = layout?.let {
                Size(bullet.width.spAt(it.fontSize) * it.density, bullet.height.spAt(it.fontSize) * it.density)
            }
            val shape = bullet.shape.wire() ?: if (layout != null && markerSize != null && bullet.shape !== RectangleShape) {
                val density = object : Density { override val density = layout.density }
                val direction = layout.directions[range.start] ?: LayoutDirection.Ltr
                bullet.shape.createOutline(markerSize, direction, density).wire(density.density)
            } else null
            shape?.let { append(",\"shape\":").append(it) }
            bullet.brush?.wire(markerSize)?.let { append(",\"brush\":").append(it) }
            if (bullet.alpha.isFinite()) append(",\"alpha\":").append(jsonNum(bullet.alpha.coerceIn(0f, 1f)))
            (bullet.drawStyle as? Stroke)?.let { append(",\"strokeStyle\":").append(it.wire()) }
            append("}}")
        }
        append(']')
    }
}

private fun TextDirection.resolve(layoutDirection: LayoutDirection): TextDirection = when (this) {
    TextDirection.Unspecified, TextDirection.Content ->
        if (layoutDirection == LayoutDirection.Rtl) TextDirection.ContentOrRtl else TextDirection.ContentOrLtr
    else -> this
}

internal data class ShaderLayout(
    val size: Size, val density: Float, val fontSize: Float, val directions: Map<Int, LayoutDirection>,
)

private class CustomAutoSizeState {
    var environment by mutableStateOf<Pair<Constraints, Density>?>(null)
    val measured = mutableStateMapOf<TextProbe, TextLayoutResult>()
}

private fun TextUnit.spAt(fontSize: Float): Float = if (isEm) value * fontSize else value

private fun TextUnit.wireUnit(): String = "[${jsonNum(value)},$isEm]"

private fun StringBuilder.appendSpan(span: SpanStyle, start: Int, end: Int, shaderSize: Size?) {
    append("{\"start\":").append(start).append(",\"end\":").append(end)
    if (span.color.isSpecified) append(",\"color\":").append(span.color.wire())
    span.brush?.wire(shaderSize)?.let { append(",\"brush\":").append(it) }
    if (span.alpha.isFinite()) append(",\"paintAlpha\":").append(jsonNum(span.alpha))
    span.shadow?.let { append(",\"textShadow\":").append(it.wire()) }
    appendInkStyle(span.drawStyle)
    span.textGeometricTransform?.let { append(",\"textTransform\":").append(it.wire()) }
    span.localeList?.firstOrNull()?.let { append(",\"textLocale\":").append(jsonString(it.toLanguageTag())) }
    if (span.fontSize.isSpecified) {
        append(if (span.fontSize.isEm) ",\"fontSizeEm\":" else ",\"fontSize\":").append(jsonNum(span.fontSize.value))
    }
    span.fontWeight?.let { append(",\"fontWeight\":").append(it.weight) }
    span.fontStyle?.let { append(",\"italic\":").append(it == FontStyle.Italic) }
    span.textDecoration?.let { append(",\"decoration\":").append(it.mask) }
    if (span.background.isSpecified) append(",\"background\":").append(span.background.wire())
    if (span.letterSpacing.isSpecified) {
        append(if (span.letterSpacing.isEm) ",\"letterSpacingEm\":" else ",\"letterSpacing\":")
        append(jsonNum(span.letterSpacing.value))
    }
    span.fontFamily?.let { append(",\"fontFamily\":").append(jsonString(it.name)) }
    span.fontFeatureSettings?.let { append(",\"fontFeatureSettings\":").append(jsonString(it)) }
    span.baselineShift?.let { append(",\"baselineShift\":").append(jsonNum(it.multiplier)) }
    append('}')
}


private fun TextGeometricTransform.wire(): String = "[${jsonNum(scaleX)},${jsonNum(skewX)}]"


private fun StringBuilder.appendParagraphBreak(lineBreak: LineBreak, hyphens: Hyphens) {
    if (lineBreak != LineBreak.Unspecified) {
        append(",\"lineBreak\":[").append(lineBreak.strategy.wire).append(',')
            .append(lineBreak.strictness.wire).append(',').append(lineBreak.wordBreak.wire).append(']')
    }
    if (hyphens != Hyphens.Unspecified) append(",\"autoHyphens\":").append(hyphens == Hyphens.Auto)
}


private fun StringBuilder.appendInkStyle(style: DrawStyle?) {
    when (style) {
        is Stroke -> append(",\"textStroke\":").append(style.wire())
        Fill -> append(",\"textFill\":true")
        null -> Unit
    }
}
