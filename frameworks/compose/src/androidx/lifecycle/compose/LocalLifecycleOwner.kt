package androidx.lifecycle.compose

import androidx.compose.runtime.ProvidableCompositionLocal
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import onesim.compose.Guest

/** The [LifecycleOwner] of the composition: the activity, or a navigation back stack entry inside a NavHost. */
val LocalLifecycleOwner: ProvidableCompositionLocal<LifecycleOwner> = staticCompositionLocalOf {
    Guest.activity ?: DetachedOwner
}

private object DetachedOwner : LifecycleOwner {
    override val lifecycle: Lifecycle = LifecycleRegistry(this)
}
