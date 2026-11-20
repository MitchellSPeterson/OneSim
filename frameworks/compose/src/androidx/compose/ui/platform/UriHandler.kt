package androidx.compose.ui.platform

import androidx.compose.runtime.staticCompositionLocalOf
import onesim.compose.Guest

fun interface UriHandler {
    fun openUri(uri: String)
}

val LocalUriHandler = staticCompositionLocalOf { UriHandler { Guest.openUri(it) } }
