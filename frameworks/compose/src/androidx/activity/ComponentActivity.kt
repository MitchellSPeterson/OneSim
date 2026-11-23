package androidx.activity

import android.os.Bundle
import androidx.lifecycle.Lifecycle
import kotlinx.coroutines.Runnable
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import onesim.compose.Guest

/**
 * The app's Activity. OneSim runs `onCreate`, `onStart` and `onResume` once; system back goes to
 * [onBackPressedDispatcher], and without an enabled callback the activity finishes ([finish]).
 */
open class ComponentActivity : LifecycleOwner, OnBackPressedDispatcherOwner {
    private val registry = LifecycleRegistry(this)
    override val lifecycle: Lifecycle get() = registry

    override val onBackPressedDispatcher: OnBackPressedDispatcher = OnBackPressedDispatcher(Runnable {
        @Suppress("DEPRECATION")
        onBackPressed()
    })

    /** Whether [finish] was called (system back with no enabled callback). */
    var isFinishing: Boolean = false
        private set

    open fun onCreate(savedInstanceState: Bundle?) {
        registry.handleLifecycleEvent(Lifecycle.Event.ON_CREATE)
    }

    open fun onStart() {
        registry.handleLifecycleEvent(Lifecycle.Event.ON_START)
    }

    open fun onResume() {
        registry.handleLifecycleEvent(Lifecycle.Event.ON_RESUME)
    }

    open fun onPause() {}
    open fun onStop() {}
    open fun onDestroy() {}

    /**
     * The default when no [OnBackPressedCallback] is enabled: finish the activity. Apps override it (deprecated in
     * AndroidX) to do something else.
     */
    @Deprecated("Use OnBackPressedDispatcher")
    open fun onBackPressed() {
        finish()
    }

    /** On a device the activity closes and the previous app shows; OneSim keeps the last frame and reports it. */
    open fun finish() {
        if (isFinishing) return
        isFinishing = true
        Guest.activityFinished()
    }
}

/** Edge-to-edge is the only mode OneSim renders (as on Android 15+ targets). */
fun ComponentActivity.enableEdgeToEdge(statusBarStyle: Any? = null, navigationBarStyle: Any? = null) {}
