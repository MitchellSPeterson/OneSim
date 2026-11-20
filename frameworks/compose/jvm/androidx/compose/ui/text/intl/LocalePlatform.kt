package androidx.compose.ui.text.intl

import java.util.Locale as JavaLocale

internal actual fun localeParts(languageTag: String): List<String> {
    val locale = JavaLocale.forLanguageTag(languageTag)
    return listOf(locale.toLanguageTag(), locale.language, locale.script, locale.country)
}

internal actual fun currentLanguageTags(): List<String> = listOf(JavaLocale.getDefault().toLanguageTag())

internal actual fun upperCase(text: String, locale: Locale): String = text.uppercase(JavaLocale.forLanguageTag(locale.toLanguageTag()))
internal actual fun lowerCase(text: String, locale: Locale): String = text.lowercase(JavaLocale.forLanguageTag(locale.toLanguageTag()))
internal actual fun titleCase(char: Char, locale: Locale): String = char.titlecase(JavaLocale.forLanguageTag(locale.toLanguageTag()))
