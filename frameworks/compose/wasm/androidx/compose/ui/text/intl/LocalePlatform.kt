package androidx.compose.ui.text.intl

@JsFun("""(tag) => {
    try {
        const locale = new Intl.Locale(tag);
        return [locale.toString(), locale.language ?? '', locale.script ?? '', locale.region ?? ''].join('|');
    } catch {
        return 'und|||';
    }
}""")
private external fun localeData(languageTag: String): String

@JsFun("() => (globalThis.navigator?.languages ?? [new Intl.DateTimeFormat().resolvedOptions().locale]).join('|')")
private external fun browserLanguages(): String

@JsFun("(text, tag) => text.toLocaleUpperCase(tag)")
private external fun localeUpper(text: String, tag: String): String

@JsFun("(text, tag) => text.toLocaleLowerCase(tag)")
private external fun localeLower(text: String, tag: String): String

internal actual fun localeParts(languageTag: String): List<String> = localeData(languageTag).split('|')
internal actual fun currentLanguageTags(): List<String> = browserLanguages().split('|').filter(String::isNotEmpty)
internal actual fun upperCase(text: String, locale: Locale): String = localeUpper(text, locale.toLanguageTag())
internal actual fun lowerCase(text: String, locale: Locale): String = localeLower(text, locale.toLanguageTag())
internal actual fun titleCase(char: Char, locale: Locale): String =
    if (char == 'i' && locale.language in listOf("tr", "az")) "İ" else char.titlecase()
