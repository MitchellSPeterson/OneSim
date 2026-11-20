package androidx.compose.ui.text

import androidx.compose.foundation.text.appendAnnotatedStyles
import androidx.compose.foundation.text.appendInlineObjects
import androidx.compose.foundation.text.appendTextStyle
import androidx.compose.foundation.text.paragraphDefaults
import androidx.compose.foundation.text.resolveTextSlots
import androidx.compose.foundation.text.ShaderLayout
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.jsonNum
import androidx.compose.ui.jsonString
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFontFamilyResolver
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.intl.LocaleList
import androidx.compose.ui.text.style.TextIndent
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.sp
import onesim.compose.Guest

class TextMeasurer(
    private val defaultFontFamilyResolver: FontFamily.Resolver,
    private val defaultDensity: Density,
    private val defaultLayoutDirection: LayoutDirection,
    private val cacheSize: Int = 8,
) {
    private val cache = LinkedHashMap<TextLayoutInput, TextLayoutResult>()

    fun measure(
        text: AnnotatedString,
        style: TextStyle = TextStyle.Default,
        overflow: TextOverflow = TextOverflow.Clip,
        softWrap: Boolean = true,
        maxLines: Int = Int.MAX_VALUE,
        placeholders: List<AnnotatedString.Range<Placeholder>> = emptyList(),
        constraints: Constraints = Constraints(),
        layoutDirection: LayoutDirection = defaultLayoutDirection,
        density: Density = defaultDensity,
        fontFamilyResolver: FontFamily.Resolver = defaultFontFamilyResolver,
        skipCache: Boolean = false,
    ): TextLayoutResult {
        val input = TextLayoutInput(text, style, maxLines, softWrap, overflow, density, layoutDirection, constraints,
            placeholders, fontFamilyResolver)
        if (!skipCache && cacheSize > 0) {
            cache.remove(input)?.let { result ->
                cache[input] = result
                return result
            }
        }
        val result = measureText(input)
        if (!skipCache && cacheSize > 0) {
            if (cache.size >= cacheSize) cache.remove(cache.keys.first())
            cache[input] = result
        }
        return result
    }

    fun measure(
        text: String,
        style: TextStyle = TextStyle.Default,
        overflow: TextOverflow = TextOverflow.Clip,
        softWrap: Boolean = true,
        maxLines: Int = Int.MAX_VALUE,
        constraints: Constraints = Constraints(),
        layoutDirection: LayoutDirection = defaultLayoutDirection,
        density: Density = defaultDensity,
        fontFamilyResolver: FontFamily.Resolver = defaultFontFamilyResolver,
        skipCache: Boolean = false,
    ): TextLayoutResult = measure(AnnotatedString(text), style, overflow, softWrap, maxLines,
        constraints = constraints, layoutDirection = layoutDirection, density = density,
        fontFamilyResolver = fontFamilyResolver, skipCache = skipCache)
}

@Composable
fun rememberTextMeasurer(cacheSize: Int = 8): TextMeasurer {
    val density = LocalDensity.current
    val direction = LocalLayoutDirection.current
    val resolver = LocalFontFamilyResolver.current
    return remember(density, direction, resolver, cacheSize) { TextMeasurer(resolver, density, direction, cacheSize) }
}

interface ParagraphIntrinsics {
    val minIntrinsicWidth: Float
    val maxIntrinsicWidth: Float
    val hasStaleResolvedFonts: Boolean get() = false
}

class MultiParagraphIntrinsics(
    val annotatedString: AnnotatedString,
    internal val style: TextStyle,
    internal val placeholders: List<AnnotatedString.Range<Placeholder>>,
    internal val density: Density,
    internal val fontFamilyResolver: FontFamily.Resolver,
    internal val softWrap: Boolean = true,
    internal val defaultLocaleList: LocaleList = LocaleList.current,
) : ParagraphIntrinsics {
    private val input = TextLayoutInput(annotatedString, style.copy(localeList = style.localeList ?: defaultLocaleList),
        Int.MAX_VALUE, softWrap, TextOverflow.Clip, density, placeholders = placeholders,
        fontFamilyResolver = fontFamilyResolver)
    override val minIntrinsicWidth: Float by lazy {
        if (softWrap) requestTextMeasurement(input, intrinsic = -1).toFloat() else maxIntrinsicWidth
    }
    override val maxIntrinsicWidth: Float by lazy { requestTextMeasurement(input, intrinsic = 1).toFloat() }

    internal fun measure(constraints: Constraints, maxLines: Int, overflow: TextOverflow): TextLayoutResult {
        require(constraints.minWidth == 0 && constraints.minHeight == 0) { "MultiParagraph requires zero minimum constraints" }
        val width = if (constraints.hasBoundedWidth) constraints.maxWidth else 0
        return measureText(input.copy(constraints = constraints.copy(minWidth = width, minHeight = 0),
            maxLines = maxLines, overflow = overflow))
    }
}

internal fun measureText(input: TextLayoutInput): TextLayoutResult =
    TextLayoutResult.decode(input, requestTextMeasurement(input))

internal fun requestTextMeasurement(
    input: TextLayoutInput,
    intrinsic: Int = 0,
    paint: String? = null,
    shaderLayout: ShaderLayout? = null,
): String {
    require(input.maxLines > 0) { "maxLines must be positive" }
    require(input.density.density > 0f && input.density.density.isFinite()) { "Invalid density" }
    require(input.density.fontScale > 0f && input.density.fontScale.isFinite()) { "Invalid font scale" }
    val text = input.text
    var end = 0
    for (range in input.placeholders.sortedBy { it.start }) {
        require(range.start >= end && range.start < range.end && range.end <= text.length) { "Invalid placeholder range" }
        require(text.text.substring(range.start, range.end).none { it == '\n' || it == '\r' }) { "Placeholder crosses a paragraph" }
        require(text.paragraphStyles.none {
            it.start in (range.start + 1) until range.end || it.end in (range.start + 1) until range.end
        }) { "Placeholder crosses a paragraph" }
        end = range.end
    }
    val (display, slots) = resolveTextSlots(text, input.placeholders)
    val scale = input.density.fontScale
    fun TextUnit.scaled() = if (isSp) (value * scale).sp else this
    fun TextIndent.scaled() = TextIndent(firstLine.scaled(), restLine.scaled())
    val style = paragraphDefaults(input.style, display, display.text).let {
        if (scale == 1f) it else it.copy(
            fontSize = (if (it.fontSize.isSpecified) it.fontSize else 14.sp).scaled(),
            lineHeight = it.lineHeight.scaled(), letterSpacing = it.letterSpacing.scaled(),
            textIndent = it.textIndent?.scaled(),
        )
    }
    val shaped = if (scale == 1f) display else AnnotatedString(display.text, display.annotations.map { range ->
        val annotation = when (val item = range.item) {
            is SpanStyle -> item.copy(fontSize = item.fontSize.scaled(), letterSpacing = item.letterSpacing.scaled())
            is ParagraphStyle -> item.copy(lineHeight = item.lineHeight.scaled(), textIndent = item.textIndent?.scaled())
            is Bullet -> item.copy(width = item.width.scaled(), height = item.height.scaled(), padding = item.padding.scaled())
            else -> item
        }
        AnnotatedString.Range(annotation, range.start, range.end, range.tag)
    })
    val links = shaped.getLinkAnnotations(0, shaped.length).filter { it.start < it.end }
    val constraints = input.constraints
    val maxLines = if (!input.softWrap && input.overflow in
        listOf(TextOverflow.Ellipsis, TextOverflow.StartEllipsis, TextOverflow.MiddleEllipsis)) 1 else input.maxLines
    val props = buildString {
        append("{\"text\":").append(jsonString(shaped.text))
        appendTextStyle(style, style.color.takeIf { it.isSpecified } ?: Color.Black, null, shaped, shaderLayout?.size,
            maxLines, 1, input.softWrap, input.overflow)
        appendInlineObjects(text.text, slots, style.fontSize.takeIf { it.isSpecified }?.value ?: 14f) {
            Placeholder(it.width.scaled(), it.height.scaled(), it.placeholderVerticalAlign)
        }
        appendAnnotatedStyles(shaped, style, shaped.text, links, shaderLayout)
        append('}')
    }
    val request = buildString {
        append("{\"density\":").append(jsonNum(input.density.density))
        append(",\"constraints\":[").append(constraints.minWidth).append(',').append(constraints.maxWidth)
            .append(',').append(constraints.minHeight).append(',').append(constraints.maxHeight).append(']')
        append(",\"props\":").append(props)
        append(",\"fonts\":").append(Guest.fontResourcesJson())
        if (paint != null) append(",\"paint\":").append(paint)
        if (intrinsic != 0) append(",\"intrinsic\":").append(intrinsic)
        append('}')
    }
    return Guest.measureTextSink(request).also { check(it.isNotEmpty()) { "OneSim could not measure text" } }
}
