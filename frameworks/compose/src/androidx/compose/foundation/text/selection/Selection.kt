package androidx.compose.foundation.text.selection

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import onesim.compose.Primitive

@Immutable
data class TextSelectionColors(val handleColor: Color, val backgroundColor: Color)

val LocalTextSelectionColors = staticCompositionLocalOf {
    val blue = Color(0xFF4286F4)
    TextSelectionColors(blue, blue.copy(alpha = 0.4f))
}

internal val LocalTextSelectable = staticCompositionLocalOf { false }

@Composable
fun SelectionContainer(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Primitive("Box", modifier, """"selectionContainer":true""") {
        CompositionLocalProvider(LocalTextSelectable provides true, content = content)
    }
}

@Composable
fun DisableSelection(content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalTextSelectable provides false, content = content)
}
