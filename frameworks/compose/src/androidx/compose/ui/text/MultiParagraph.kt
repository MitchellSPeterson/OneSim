package androidx.compose.ui.text

import androidx.compose.foundation.text.ShaderLayout
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.drawscope.DrawStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.intl.LocaleList
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.ResolvedTextDirection
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection

/** A shaped group of paragraphs created directly or returned by text layout. */
class MultiParagraph internal constructor(
    private val layout: TextLayoutResult,
    precomputedIntrinsics: MultiParagraphIntrinsics? = null,
) {
    constructor(
        intrinsics: MultiParagraphIntrinsics,
        constraints: Constraints,
        maxLines: Int = Int.MAX_VALUE,
        overflow: TextOverflow = TextOverflow.Clip,
    ) : this(intrinsics.measure(constraints, maxLines, overflow), intrinsics)

    constructor(
        annotatedString: AnnotatedString,
        style: TextStyle,
        constraints: Constraints,
        density: Density,
        fontFamilyResolver: FontFamily.Resolver,
        placeholders: List<AnnotatedString.Range<Placeholder>> = emptyList(),
        maxLines: Int = Int.MAX_VALUE,
        overflow: TextOverflow = TextOverflow.Clip,
    ) : this(MultiParagraphIntrinsics(annotatedString, style, placeholders, density, fontFamilyResolver),
        constraints, maxLines, overflow)

    constructor(
        annotatedString: AnnotatedString,
        style: TextStyle,
        constraints: Constraints,
        density: Density,
        fontFamilyResolver: FontFamily.Resolver,
        defaultLocaleList: LocaleList,
        placeholders: List<AnnotatedString.Range<Placeholder>> = emptyList(),
        maxLines: Int = Int.MAX_VALUE,
        overflow: TextOverflow = TextOverflow.Clip,
    ) : this(MultiParagraphIntrinsics(annotatedString, style, placeholders, density, fontFamilyResolver,
        defaultLocaleList = defaultLocaleList), constraints, maxLines, overflow)

    val intrinsics: MultiParagraphIntrinsics by lazy {
        precomputedIntrinsics ?: layout.layoutInput.let {
            MultiParagraphIntrinsics(it.text, it.style, it.placeholders, it.density, it.fontFamilyResolver, it.softWrap)
        }
    }
    val maxLines: Int get() = layout.layoutInput.maxLines
    val width: Float get() = layout.size.width.toFloat()
    val height: Float get() = if (lineCount == 0) 0f else getLineBottom(lineCount - 1)
    val didExceedMaxLines: Boolean get() = layout.didExceedMaxLines
    val lineCount: Int get() = layout.lineCount
    val firstBaseline: Float get() = layout.firstBaseline
    val lastBaseline: Float get() = layout.lastBaseline
    val placeholderRects: List<Rect?> get() = layout.placeholderRects

    fun paint(
        canvas: Canvas,
        color: Color = Color.Unspecified,
        shadow: Shadow? = null,
        decoration: TextDecoration? = null,
        drawStyle: DrawStyle? = null,
        blendMode: BlendMode = BlendMode.SrcOver,
    ) {
        val style = layout.layoutInput.style
        paint(canvas, style.copy(
            color = color.takeIf { it.isSpecified } ?: style.color,
            brush = style.brush.takeUnless { color.isSpecified },
            shadow = shadow ?: style.shadow,
            textDecoration = decoration ?: style.textDecoration,
            drawStyle = drawStyle ?: style.drawStyle,
        ), blendMode)
    }

    fun paint(
        canvas: Canvas,
        brush: Brush,
        alpha: Float = Float.NaN,
        shadow: Shadow? = null,
        decoration: TextDecoration? = null,
        drawStyle: DrawStyle? = null,
        blendMode: BlendMode = BlendMode.SrcOver,
    ) {
        val style = layout.layoutInput.style
        paint(canvas, style.copy(
            brush = brush,
            alpha = alpha.takeIf { it.isFinite() } ?: style.alpha,
            shadow = shadow ?: style.shadow,
            textDecoration = decoration ?: style.textDecoration,
            drawStyle = drawStyle ?: style.drawStyle,
        ), blendMode)
    }

    private fun paint(canvas: Canvas, style: TextStyle, blendMode: BlendMode) {
        require(blendMode == BlendMode.SrcOver)
        val input = layout.layoutInput
        val paint = ShaderLayout(
            Size(width, height), input.density.density,
            (style.fontSize.takeIf { it.isSpecified }?.value ?: 14f) * input.density.fontScale,
            input.text.bullets.associate { bullet ->
                val rtl = layout.getParagraphDirection(bullet.start) == ResolvedTextDirection.Rtl
                bullet.start to if (rtl) LayoutDirection.Rtl else LayoutDirection.Ltr
            },
        )
        canvas.compositeText(requestTextMeasurement(input.copy(style = style), paint = canvas.textPaintWire(), shaderLayout = paint))
    }

    fun getLineStart(lineIndex: Int) = layout.getLineStart(lineIndex)
    fun getLineEnd(lineIndex: Int, visibleEnd: Boolean = false) = layout.getLineEnd(lineIndex, visibleEnd)
    fun getLineTop(lineIndex: Int) = layout.getLineTop(lineIndex)
    fun getLineBottom(lineIndex: Int) = layout.getLineBottom(lineIndex)
    fun getLineLeft(lineIndex: Int) = layout.getLineLeft(lineIndex)
    fun getLineRight(lineIndex: Int) = layout.getLineRight(lineIndex)
    fun getLineBaseline(lineIndex: Int) = layout.getLineBaseline(lineIndex)
    fun getLineHeight(lineIndex: Int) = getLineBottom(lineIndex) - getLineTop(lineIndex)
    fun getLineWidth(lineIndex: Int) = getLineRight(lineIndex) - getLineLeft(lineIndex)
    fun isLineEllipsized(lineIndex: Int) = layout.isLineEllipsized(lineIndex)
    fun getLineForOffset(offset: Int) = layout.getLineForOffset(offset)
    fun getLineForVerticalPosition(vertical: Float) = layout.getLineForVerticalPosition(vertical)
    fun getOffsetForPosition(position: Offset) = layout.getOffsetForPosition(position)
    fun getParagraphDirection(offset: Int) = layout.getParagraphDirection(offset)
    fun getBidiRunDirection(offset: Int) = layout.getBidiRunDirection(offset)
    fun getHorizontalPosition(offset: Int, usePrimaryDirection: Boolean) = layout.getHorizontalPosition(offset, usePrimaryDirection)
    fun getBoundingBox(offset: Int) = layout.getBoundingBox(offset)
    fun getCursorRect(offset: Int) = layout.getCursorRect(offset)
    fun getWordBoundary(offset: Int) = layout.getWordBoundary(offset)
    fun getPathForRange(start: Int, end: Int) = layout.getPathForRange(start, end)
    fun getRangeForRect(rect: Rect, granularity: TextGranularity, inclusionStrategy: TextInclusionStrategy) =
        layout.getRangeForRect(rect, granularity, inclusionStrategy)

    fun fillBoundingBoxes(range: TextRange, array: FloatArray, arrayStart: Int): FloatArray {
        require(range.min >= 0 && range.max <= layout.layoutInput.text.length)
        require(arrayStart in 0..array.size && range.length <= (array.size - arrayStart) / 4)
        var index = arrayStart
        for (offset in range.min until range.max) {
            val bounds = getBoundingBox(offset)
            array[index++] = bounds.left
            array[index++] = bounds.top
            array[index++] = bounds.right
            array[index++] = bounds.bottom
        }
        return array
    }
}
