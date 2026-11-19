package androidx.activity

import android.os.Bundle

/** The app's Activity. OneSim runs `onCreate` once; other lifecycle callbacks are not delivered yet. */
open class ComponentActivity {
    open fun onCreate(savedInstanceState: Bundle?) {}
    open fun onStart() {}
    open fun onResume() {}
    open fun onPause() {}
    open fun onStop() {}
    open fun onDestroy() {}
    open fun finish() {}
}

/** Edge-to-edge is the only mode OneSim renders (as on Android 15+ targets). */
fun ComponentActivity.enableEdgeToEdge(statusBarStyle: Any? = null, navigationBarStyle: Any? = null) {}
