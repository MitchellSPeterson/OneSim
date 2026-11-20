package onesim.tests

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.ClickableText
import androidx.compose.foundation.text.selection.DisableSelection
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.TtsAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextGranularity
import androidx.compose.ui.text.TextInclusionStrategy
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.Bullet
import androidx.compose.ui.text.capitalize
import androidx.compose.ui.text.decapitalize
import androidx.compose.ui.text.intl.Locale
import androidx.compose.ui.text.intl.LocaleList
import androidx.compose.ui.text.toLowerCase
import androidx.compose.ui.text.toUpperCase
import androidx.compose.ui.unit.em
import androidx.compose.ui.text.withAnnotation
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.withLink

@Suppress("DEPRECATION")
@Composable
fun TestText() {
    val annotated = remember {
        val german = buildAnnotatedString {
            append("😀")
            withAnnotation("id", "item") { append("Straße") }
            append("!")
        }.toUpperCase(LocaleList("de-DE"))
        check(german.text == "😀STRASSE!")
        check(german.getStringAnnotations("id", 0, german.length).single().let { it.start == 2 && it.end == 9 })
        val turkish = Locale("tr-TR")
        check(turkish.language == "tr" && turkish.region == "TR")
        check(AnnotatedString("İI").toLowerCase(turkish).text == "iı")
        check(AnnotatedString("istanbul").capitalize(turkish).text == "İstanbul")
        check(AnnotatedString("Isparta").decapitalize(turkish).text == "ısparta")
        val nested = buildAnnotatedString {
            withBulletList(indentation = 1.em) {
                withBulletListItem { append("outer") }
                withBulletList(indentation = 2.em) {
                    withBulletListItem { append("inner") }
                }
            }
        }
        check(nested.text == "outerinner")
        check(nested.paragraphStyles.any { it.start == 5 && it.item.textIndent?.firstLine == 3.em })
        var bullets = 0
        nested.subSequence(5, nested.length).mapAnnotations {
            if (it.item is Bullet) bullets++
            it
        }
        check(bullets == 1)
        val link = buildAnnotatedString {
            append(text = "😀")
            withAnnotation("route", "details") {
                withStyle(SpanStyle(color = Color.Blue)) { append("Go") }
            }
        }
        val text = buildAnnotatedString { append("A"); append(link) }
        val tail = text.subSequence(3, 5)
        check(tail.text == "Go")
        check(tail.spanStyles.single().let { it.start == 0 && it.end == 2 && it.item.color == Color.Blue })
        check(tail.getStringAnnotations("route", 0, 2).single().item == "details")
        check(AnnotatedString("styled", spanStyle = SpanStyle(color = Color.Blue)).spanStyles.single().end == 6)
        val annotations = buildAnnotatedString {
            withLink(LinkAnnotation.Url("https://example.org")) {
                withAnnotation(TtsAnnotation.Verbatim("example")) { append("Ex") }
            }
        }
        check(annotations.getLinkAnnotations(0, 2).single().item == LinkAnnotation.Url("https://example.org"))
        check(annotations.subSequence(1, 2).getTtsAnnotations(0, 1).single().item == TtsAnnotation.Verbatim("example"))
        text
    }
    var offset by remember { mutableIntStateOf(-1) }
    var lineCount by remember { mutableIntStateOf(0) }
    SelectionContainer {
        Column {
            ClickableText(annotated, onTextLayout = { result ->
                check(result.layoutInput.text == annotated)
                check(result.size.width > 0)
                check(result.getLineStart(0) == 0)
                val box = result.getBoundingBox(0)
                check(box.width > 0)
                check(result.multiParagraph.getLineHeight(0) > 0)
                val flat = result.multiParagraph.fillBoundingBoxes(TextRange(0, 1), FloatArray(4), 0)
                check(flat.contentEquals(floatArrayOf(box.left, box.top, box.right, box.bottom)))
                check(result.getPathForRange(0, 1).getBounds() == box)
                check(result.multiParagraph.getRangeForRect(
                    box, TextGranularity.Character, TextInclusionStrategy.ContainsAll,
                ) == TextRange(0, 1))
                lineCount = result.lineCount
            }) { offset = it }
            DisableSelection {
                val route = annotated.getStringAnnotations("route", offset, offset).firstOrNull()?.item
                BasicText("offset=$offset route=$route")
                BasicText("Layout lines: $lineCount")
            }
        }
    }
}
