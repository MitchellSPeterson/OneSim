package androidx.activity.compose

import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionContext
import simcore.compose.Guest

fun ComponentActivity.setContent(parent: CompositionContext? = null, content: @Composable () -> Unit) {
    Guest.setContent(content)
}
