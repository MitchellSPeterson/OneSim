package onesim.compose

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalContext
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.currentCompositionLocalContext
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue

/**
 * Window-level layers (what AndroidX puts in a dialog or popup window): content composed by [Overlay] anywhere in the
 * tree is drawn after the app's content, over the whole screen, with the composition locals of the place that
 * declared it. The guest composes [OverlayHost] after the app's root content.
 */
object Overlays {
    internal class Entry(context: CompositionLocalContext, content: @Composable () -> Unit) {
        var context by mutableStateOf(context)
        var content by mutableStateOf(content)
    }

    internal val entries = mutableStateListOf<Entry>()
}

/** Show [content] in a full-screen layer over the app (like a dialog window) while this call is in the composition. */
@Composable
fun Overlay(content: @Composable () -> Unit) {
    val context = currentCompositionLocalContext
    val entry = remember { Overlays.Entry(context, content) }
    SideEffect {
        entry.context = context
        entry.content = content
    }
    DisposableEffect(entry) {
        Overlays.entries.add(entry)
        onDispose { Overlays.entries.remove(entry) }
    }
}

/** The overlay layers, in the order they were shown (the latest on top). */
@Composable
fun OverlayHost() {
    for (e in Overlays.entries) {
        key(e) { CompositionLocalProvider(e.context) { e.content() } }
    }
}
