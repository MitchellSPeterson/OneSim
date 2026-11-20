package androidx.compose.ui.text

import androidx.compose.ui.text.intl.Locale
import androidx.compose.ui.text.intl.LocaleList
import androidx.compose.ui.text.intl.lowerCase
import androidx.compose.ui.text.intl.titleCase
import androidx.compose.ui.text.intl.upperCase

fun AnnotatedString.toUpperCase(locale: Locale): AnnotatedString = mapCase { part, _ -> upperCase(part, locale) }
fun AnnotatedString.toLowerCase(locale: Locale): AnnotatedString = mapCase { part, _ -> lowerCase(part, locale) }
fun AnnotatedString.capitalize(locale: Locale): AnnotatedString = mapCase { part, first ->
    if (first) part.replaceFirstChar { if (it.isLowerCase()) titleCase(it, locale) else it.toString() } else part
}
fun AnnotatedString.decapitalize(locale: Locale): AnnotatedString = mapCase { part, first ->
    if (first) part.replaceFirstChar { lowerCase(it.toString(), locale) } else part
}

fun AnnotatedString.toUpperCase(localeList: LocaleList = LocaleList.current): AnnotatedString = toUpperCase(localeList.firstOrNull() ?: Locale.current)
fun AnnotatedString.toLowerCase(localeList: LocaleList = LocaleList.current): AnnotatedString = toLowerCase(localeList.firstOrNull() ?: Locale.current)
fun AnnotatedString.capitalize(localeList: LocaleList = LocaleList.current): AnnotatedString = capitalize(localeList.firstOrNull() ?: Locale.current)
fun AnnotatedString.decapitalize(localeList: LocaleList = LocaleList.current): AnnotatedString = decapitalize(localeList.firstOrNull() ?: Locale.current)

/** Map each interval between annotation edges once. A contextual mapping stays within its interval. */
private fun AnnotatedString.mapCase(transform: (String, Boolean) -> String): AnnotatedString {
    val boundaries = (annotations.flatMap { listOf(it.start, it.end) } + listOf(0, length)).distinct().sorted()
    val offsets = mutableMapOf(0 to 0)
    val converted = buildString {
        for ((start, end) in boundaries.zipWithNext()) {
            append(transform(text.substring(start, end), start == 0))
            offsets[end] = length
        }
    }
    return AnnotatedString(converted, annotations = annotations.map {
        AnnotatedString.Range(it.item, offsets.getValue(it.start), offsets.getValue(it.end), it.tag)
    })
}
