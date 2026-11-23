package androidx.compose.ui.window

import androidx.compose.runtime.Immutable

/** Behaviour of a dialog window: dismissal on back / outside taps and whether it takes the platform's default width. */
@Immutable
class DialogProperties(
    val dismissOnBackPress: Boolean = true,
    val dismissOnClickOutside: Boolean = true,
    val usePlatformDefaultWidth: Boolean = true,
    val decorFitsSystemWindows: Boolean = true,
) {
    override fun equals(other: Any?) = other is DialogProperties && dismissOnBackPress == other.dismissOnBackPress &&
        dismissOnClickOutside == other.dismissOnClickOutside && usePlatformDefaultWidth == other.usePlatformDefaultWidth &&
        decorFitsSystemWindows == other.decorFitsSystemWindows
    override fun hashCode() = listOf(dismissOnBackPress, dismissOnClickOutside, usePlatformDefaultWidth, decorFitsSystemWindows).hashCode()
}
