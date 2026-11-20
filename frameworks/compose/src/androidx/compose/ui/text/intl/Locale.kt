package androidx.compose.ui.text.intl

class Locale(languageTag: String) {
    private val parts = localeParts(languageTag)
    val language: String get() = parts[1]
    val script: String get() = parts[2]
    val region: String get() = parts[3]

    fun toLanguageTag(): String = parts[0]

    override fun equals(other: Any?): Boolean = other is Locale && toLanguageTag() == other.toLanguageTag()
    override fun hashCode(): Int = toLanguageTag().hashCode()
    override fun toString(): String = toLanguageTag()

    companion object {
        val current: Locale get() = Locale(currentLanguageTags().firstOrNull() ?: "und")
    }
}

class LocaleList(val localeList: List<Locale>) : Collection<Locale> by localeList {
    constructor(languageTags: String) : this(languageTags.split(',').map(String::trim).filter(String::isNotEmpty).map(::Locale))
    constructor(vararg locales: Locale) : this(locales.toList())
    operator fun get(index: Int): Locale = localeList[index]

    override fun equals(other: Any?): Boolean = other is LocaleList && localeList == other.localeList
    override fun hashCode(): Int = localeList.hashCode()
    override fun toString(): String = localeList.toString()

    companion object {
        val Empty = LocaleList(emptyList())
        val current: LocaleList get() = LocaleList(currentLanguageTags().map(::Locale))
    }
}

internal expect fun localeParts(languageTag: String): List<String>
internal expect fun currentLanguageTags(): List<String>
internal expect fun upperCase(text: String, locale: Locale): String
internal expect fun lowerCase(text: String, locale: Locale): String
internal expect fun titleCase(char: Char, locale: Locale): String
