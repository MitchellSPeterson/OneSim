package com.example.text

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.ClickableText
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.foundation.text.TextAutoSize
import androidx.compose.foundation.text.modifiers.TextAutoSizeLayoutScope
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.foundation.text.selection.DisableSelection
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.ImageShader
import androidx.compose.ui.graphics.LinearGradientShader
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.Shader
import androidx.compose.ui.graphics.ShaderBrush
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StampedPathEffectStyle
import androidx.compose.ui.graphics.TileMode
import androidx.compose.ui.graphics.drawscope.Fill
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.res.imageResource
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFontFamilyResolver
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.Bullet
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.MultiParagraph
import androidx.compose.ui.text.ParagraphStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.intl.LocaleList
import androidx.compose.ui.text.toUpperCase
import androidx.compose.ui.text.style.TextGeometricTransform
import androidx.compose.ui.text.style.Hyphens
import androidx.compose.ui.text.style.LineBreak
import androidx.compose.ui.text.style.ResolvedTextDirection
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDirection
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.withLink
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import dev.simcore.reference.R

class TextActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { TextDemo() }
    }
}

class InlineTextActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { InlineTextDemo() }
    }
}

@Composable
fun InlineTextDemo() {
    var measured by remember { mutableIntStateOf(0) }
    MaterialTheme {
        Column(Modifier.padding(start = 24.dp, top = 104.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            val line = buildAnnotatedString {
                append("Inline ")
                appendInlineContent("badge", alternateText = "color swatch")
                append(" composable")
            }
            Text(
                line,
                inlineContent = mapOf("badge" to InlineTextContent(Placeholder(24.sp, 16.sp, PlaceholderVerticalAlign.Center)) {
                    Box(Modifier.size(width = 24.dp, height = 16.dp).background(Color(0xFF36569B)))
                }),
                onTextLayout = { measured = it.placeholderRects.singleOrNull()?.width?.toInt() ?: 0 },
            )
            Text("Inline width: $measured px")
        }
    }
}

@Suppress("DEPRECATION")
@Composable
fun TextDemo() {
    var offset by remember { mutableIntStateOf(-1) }
    val headline = buildAnnotatedString {
        append("Compose ")
        withStyle(SpanStyle(color = Color(0xFF36569B), fontWeight = FontWeight.Bold)) {
            append("text")
        }
    }
    MaterialTheme {
        Column(
            Modifier.fillMaxSize().padding(start = 24.dp, top = 104.dp, end = 24.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text(headline, fontSize = 22.sp)
            ClickableText(AnnotatedString("Tap a character"), style = TextStyle(fontSize = 18.sp)) { offset = it }
            BasicText("Tapped offset: $offset")
            SelectionContainer {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("Select this text")
                    DisableSelection { Text("Skip this text") }
                    Text("Select this too")
                }
            }
        }
    }
}


class RichTextActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { RichTextDemo() }
    }
}

@Composable
fun RichTextDemo() {
    var fittedSize by remember { mutableIntStateOf(0) }
    var linkActivations by remember { mutableIntStateOf(0) }
    var customSize by remember { mutableIntStateOf(0) }
    MaterialTheme {
        Column(Modifier.padding(start = 24.dp, top = 104.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            val list = buildAnnotatedString {
                withBulletList(indentation = 1.em) {
                    withBulletListItem { append("Check out the project") }
                    withBulletListItem { append("Read the contribution guide") }
                    withBulletList(indentation = 1.em) {
                        val marker = Bullet.Default.copy(
                            shape = RoundedCornerShape(1.dp),
                            brush = SolidColor(Color(0xFF36569B)),
                        )
                        withBulletListItem(bullet = marker) { append("Run the tests locally") }
                    }
                }
            }
            SelectionContainer {
                Text(list, fontSize = 18.sp)
            }
            val translated = buildAnnotatedString {
                append("Grüße aus ")
                withStyle(SpanStyle(color = Color(0xFF36569B), fontWeight = FontWeight.Bold)) { append("Straße") }
            }
            Text(translated.toUpperCase(LocaleList("de-DE")))
            BasicText(
                "Fits the space",
                modifier = Modifier.size(width = 148.dp, height = 32.dp),
                style = TextStyle(color = Color(0xFF36569B)),
                autoSize = TextAutoSize.StepBased(10.sp, 38.sp, 1.sp),
                maxLines = 1,
                onTextLayout = { fittedSize = it.layoutInput.style.fontSize.value.toInt() },
            )
            Text("Fitted size: $fittedSize sp")
            val link = buildAnnotatedString {
                withLink(LinkAnnotation.Clickable(
                    tag = "details",
                    styles = TextLinkStyles(
                        style = SpanStyle(color = Color(0xFF36569B)),
                        focusedStyle = SpanStyle(background = Color(0xFFFFDF91)),
                        hoveredStyle = SpanStyle(background = Color(0xFFD8E3FF)),
                        pressedStyle = SpanStyle(background = Color(0xFFABCBFF)),
                    ),
                    linkInteractionListener = { linkActivations++ },
                )) { append("Project details") }
            }
            Text(link)
            Text("Activated: $linkActivations")
            val sampleFont = remember { FontFamily(Font(R.font.sample_inter)) }
            Text(
                buildAnnotatedString {
                    append("Bundled font ")
                    withStyle(SpanStyle(fontFeatureSettings = "'liga' off")) { append("office") }
                },
                fontFamily = sampleFont,
            )
            BasicText(
                "Custom layout",
                modifier = Modifier.size(width = 119.dp, height = 29.dp),
                autoSize = MeasuredSizes,
                maxLines = 1,
                onTextLayout = { customSize = it.layoutInput.style.fontSize.value.toInt() },
            )
            Text("Custom size: $customSize sp")
        }
    }
}

private data object MeasuredSizes : TextAutoSize {
    override fun TextAutoSizeLayoutScope.getFontSize(constraints: Constraints, text: AnnotatedString): TextUnit =
        listOf(30.sp, 27.sp, 24.sp, 21.sp, 18.sp)
            .firstOrNull { !performLayout(constraints, text, it).hasVisualOverflow } ?: 18.sp
}

class TextEffectsActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { TextEffectsDemo() }
    }
}

@Composable
fun TextEffectsDemo() {
    val blue = Color(0xFF224F91)
    val plum = Color(0xFF892D66)
    val amber = Color(0xFFD27C15)
    MaterialTheme {
        Column(Modifier.padding(start = 24.dp, top = 104.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
            val headline = buildAnnotatedString {
                withStyle(SpanStyle(brush = Brush.horizontalGradient(listOf(blue, plum)), alpha = 0.9f)) {
                    append("Gradient text")
                }
            }
            Text(headline, fontSize = 28.sp)
            val detail = buildAnnotatedString {
                append("Soft ")
                withStyle(SpanStyle(color = blue, shadow = Shadow(blue.copy(alpha = 0.7f), Offset(3f, 4f), 6f))) {
                    append("shadow")
                }
            }
            Text(detail, fontSize = 24.sp)
            val customBrush = remember {
                object : ShaderBrush() {
                    override fun createShader(size: Size): Shader =
                        LinearGradientShader(Offset.Zero, Offset(size.width, size.height), listOf(plum, blue))
                }
            }
            Text(buildAnnotatedString {
                withStyle(SpanStyle(brush = customBrush)) { append("Measured shader") }
            }, fontSize = 22.sp)
            val bullet = Bullet.Default.copy(
                width = 0.75.em,
                height = 0.75.em,
                brush = Brush.radialGradient(listOf(amber, plum)),
                drawStyle = Stroke(width = 2.5f, cap = StrokeCap.Round, pathEffect = PathEffect.dashPathEffect(floatArrayOf(5f, 3f))),
            )
            Text(buildAnnotatedString {
                withBulletList(indentation = 1.25.em, bullet = bullet) {
                    withBulletListItem { append("Stroked marker") }
                    withBulletListItem(bullet.copy(shape = DiamondMarker, drawStyle = Fill)) { append("Custom marker") }
                }
            }, fontSize = 20.sp)
            Text("Bundled font office", fontSize = 20.sp, fontFamily = remember { FontFamily(Font(R.font.sample_family)) })
            Text(buildAnnotatedString {
                withStyle(SpanStyle(color = blue, textGeometricTransform = TextGeometricTransform(scaleX = 1.35f, skewX = -0.18f))) {
                    append("Wide and skewed")
                }
            }, fontSize = 22.sp)
            Text(buildAnnotatedString {
                withStyle(SpanStyle(color = blue, drawStyle = Stroke(width = 1.5f))) { append("Outlined text") }
            }, fontSize = 20.sp)
            BasicText(
                "Extraordinary typography",
                modifier = Modifier.size(width = 83.dp, height = 73.dp),
                style = TextStyle(fontSize = 18.sp, lineBreak = LineBreak.Paragraph, hyphens = Hyphens.Auto),
            )
            BasicText(
                "Extraordinary typography",
                modifier = Modifier.size(width = 104.dp, height = 24.dp),
                style = TextStyle(fontSize = 16.sp, lineBreak = LineBreak.Paragraph, hyphens = Hyphens.Auto),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

class TextPaintActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { TextPaintDemo() }
    }
}

@Composable
fun TextPaintDemo() {
    val blue = Color(0xFF224F91)
    val plum = Color(0xFF892D66)
    val amber = Color(0xFFD27C15)
    MaterialTheme {
        Column(Modifier.padding(start = 24.dp, top = 104.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
            val tile = ImageBitmap.imageResource(R.drawable.text_tile)
            val imageBrush = remember(tile) {
                ShaderBrush(ImageShader(tile, tileModeX = TileMode.Repeated, tileModeY = TileMode.Mirror))
            }
            Text(buildAnnotatedString {
                withStyle(SpanStyle(brush = imageBrush, fontWeight = FontWeight.Bold)) { append("Bitmap shader") }
            }, fontSize = 28.sp)
            val bullet = Bullet.Default.copy(
                width = 0.75.em,
                height = 0.75.em,
                brush = Brush.radialGradient(listOf(amber, plum)),
            )
            Text(buildAnnotatedString {
                withBulletList(indentation = 1.25.em, bullet = bullet) {
                    val rounded = PathEffect.cornerPathEffect(radius = 4f)
                    val chained = PathEffect.chainPathEffect(
                        outer = PathEffect.dashPathEffect(floatArrayOf(5f, 2f)),
                        inner = rounded,
                    )
                    withBulletListItem(bullet.copy(shape = DiamondMarker, drawStyle = Stroke(2.5f, pathEffect = chained))) {
                        append("Rounded, dashed marker")
                    }
                    val stamp = Path().apply {
                        moveTo(0f, -2.5f)
                        lineTo(4f, 0f)
                        lineTo(0f, 2.5f)
                        close()
                    }
                    val stamped = PathEffect.stampedPathEffect(stamp, 8f, 2f, StampedPathEffectStyle.Rotate)
                    withBulletListItem(bullet.copy(brush = imageBrush, drawStyle = Stroke(pathEffect = stamped))) {
                        append("Stamped marker")
                    }
                }
            }, fontSize = 20.sp)
            val sample = buildAnnotatedString {
                withStyle(SpanStyle(color = plum, fontWeight = FontWeight.Bold)) { append("Large") }
                append(" sample [chip] wraps")
            }
            val placeholder = AnnotatedString.Range(
                Placeholder(16.sp, 12.sp, PlaceholderVerticalAlign.AboveBaseline), 13, 19,
            )
            val density = LocalDensity.current
            val bounds = Constraints(maxWidth = with(density) { 120.dp.toPx().toInt() })
            val style = TextStyle(fontSize = 18.sp)
            val measurer = rememberTextMeasurer()
            val result = measurer.measure(sample, style, constraints = bounds, placeholders = listOf(placeholder))
            val paragraph = MultiParagraph(
                sample, style, bounds, density, LocalFontFamilyResolver.current, placeholders = listOf(placeholder),
            )
            check(result.lineCount == paragraph.lineCount && result.placeholderRects == paragraph.placeholderRects)
            check(paragraph.intrinsics.minIntrinsicWidth < paragraph.intrinsics.maxIntrinsicWidth)
            remember(paragraph) {
                val bitmap = ImageBitmap(paragraph.width.toInt(), 80)
                Canvas(bitmap).apply {
                    save()
                    translate(4f, 4f)
                    clipRect(0f, 0f, 40f, 30f)
                    paragraph.paint(this, brush = Brush.horizontalGradient(listOf(blue, plum)))
                    restore()
                }
                val pixels = IntArray(bitmap.width * bitmap.height)
                bitmap.readPixels(pixels)
                check(pixels.any { it ushr 24 != 0 })
                check(pixels.withIndex().none { (index, pixel) ->
                    pixel ushr 24 != 0 && (index % bitmap.width !in 4 until 44 || index / bitmap.width !in 4 until 34)
                })
            }
            val layoutWidth = if (paragraph.getBoundingBox(0).width > 0f) "Measured" else "Empty"
            Text("$layoutWidth ${result.lineCount} lines, ${result.placeholderRects.count { it != null }} placeholder", color = blue)
        }
    }
}

class TextDirectionActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { TextDirectionDemo() }
    }
}

@Composable
fun TextDirectionDemo() {
    val box = Modifier.size(width = 260.dp, height = 38.dp)
    val style = TextStyle(fontSize = 22.sp, color = Color(0xFF224F91))
    MaterialTheme {
        Column(Modifier.padding(start = 24.dp, top = 104.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            BasicText("ABC (123)", box, style.copy(textDirection = TextDirection.Rtl, textAlign = TextAlign.Start))
            BasicText("ABC (123)", box, style.copy(textDirection = TextDirection.Rtl, textAlign = TextAlign.End))
            CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Rtl) {
                BasicText("123 - 456", box, style.copy(textDirection = TextDirection.Content))
            }
            BasicText(buildAnnotatedString {
                withStyle(ParagraphStyle(textDirection = TextDirection.ContentOrRtl, textAlign = TextAlign.Left)) {
                    append("[123 - 456]")
                }
            }, box, style)
            val measurer = rememberTextMeasurer()
            val result = measurer.measure("ABC (123)", style.copy(textDirection = TextDirection.Rtl))
            check(result.getParagraphDirection(0) == ResolvedTextDirection.Rtl)
            check(result.getBidiRunDirection(0) == ResolvedTextDirection.Ltr)
            check(result.getLineStart(0) == 0 && result.getLineEnd(0) == 9)
            val density = LocalDensity.current
            val constrained = measurer.measure(
                "A paragraph long enough to wrap onto multiple lines.",
                style,
                overflow = TextOverflow.Ellipsis,
                constraints = Constraints(
                    maxWidth = with(density) { 140.dp.toPx().toInt() },
                    maxHeight = with(density) { 28.dp.toPx().toInt() },
                ),
            )
            check(constrained.lineCount == 1 && constrained.isLineEllipsized(0))
        }
    }
}

private object DiamondMarker : Shape {
    override fun createOutline(size: Size, layoutDirection: LayoutDirection, density: Density): Outline =
        Outline.Generic(Path().apply {
            moveTo(size.width / 2f, 0f)
            lineTo(size.width, size.height / 2f)
            lineTo(size.width / 2f, size.height)
            lineTo(0f, size.height / 2f)
            close()
        })
}
