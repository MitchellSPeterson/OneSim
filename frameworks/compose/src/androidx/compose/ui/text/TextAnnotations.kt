package androidx.compose.ui.text

fun interface LinkInteractionListener {
    fun onClick(link: LinkAnnotation)
}

data class TextLinkStyles(
    val style: SpanStyle? = null,
    val focusedStyle: SpanStyle? = null,
    val hoveredStyle: SpanStyle? = null,
    val pressedStyle: SpanStyle? = null,
)

sealed class LinkAnnotation : AnnotatedString.Annotation {
    abstract val styles: TextLinkStyles?
    abstract val linkInteractionListener: LinkInteractionListener?

    data class Url(
        val url: String,
        override val styles: TextLinkStyles? = null,
        override val linkInteractionListener: LinkInteractionListener? = null,
    ) : LinkAnnotation()

    data class Clickable(
        val tag: String,
        override val styles: TextLinkStyles? = null,
        override val linkInteractionListener: LinkInteractionListener? = null,
    ) : LinkAnnotation()
}

sealed class TtsAnnotation : AnnotatedString.Annotation {
    data class Verbatim(val verbatim: String) : TtsAnnotation()
}

@Deprecated("Use LinkAnnotation.Url instead.")
data class UrlAnnotation(val url: String) : AnnotatedString.Annotation
