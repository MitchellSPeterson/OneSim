package androidx.activity

import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.LifecycleOwner

/**
 * A system back event (AndroidX `BackEventCompat`): the finger in window pixels, gesture [progress] 0...1 and the edge
 * the swipe started from.
 */
class BackEventCompat(
    val touchX: Float,
    val touchY: Float,
    val progress: Float,
    val swipeEdge: Int,
    val frameTimeMillis: Long = 0L,
) {
    init {
        require(progress in 0f..1f) { "progress must be in 0..1, was $progress" }
    }

    override fun toString(): String =
        "BackEventCompat{touchX=$touchX, touchY=$touchY, progress=$progress, swipeEdge=$swipeEdge, frameTimeMillis=$frameTimeMillis}"

    companion object {
        const val EDGE_LEFT = 0
        const val EDGE_RIGHT = 1
        const val EDGE_NONE = 2
    }
}

/**
 * A back handler registered with an [OnBackPressedDispatcher]. The most recently added enabled callback handles back;
 * a predictive back gesture calls [handleOnBackStarted], [handleOnBackProgressed]..., then [handleOnBackPressed]
 * (commit) or [handleOnBackCancelled].
 */
abstract class OnBackPressedCallback(enabled: Boolean) {
    private val cancellables = ArrayList<Cancellable>()
    internal var enabledChangedCallback: (() -> Unit)? = null

    var isEnabled: Boolean = enabled
        set(value) {
            field = value
            enabledChangedCallback?.invoke()
        }

    /** Removes this callback from every dispatcher it was added to. */
    fun remove() {
        for (c in cancellables.toList()) c.cancel()
    }

    open fun handleOnBackStarted(backEvent: BackEventCompat) {}

    open fun handleOnBackProgressed(backEvent: BackEventCompat) {}

    abstract fun handleOnBackPressed()

    open fun handleOnBackCancelled() {}

    internal fun addCancellable(c: Cancellable) {
        cancellables.add(c)
    }

    internal fun removeCancellable(c: Cancellable) {
        cancellables.remove(c)
    }
}

/** A handle that unregisters something. */
fun interface Cancellable {
    fun cancel()
}

/**
 * Dispatches system back to [OnBackPressedCallback]s, last added first; without an enabled one, [fallbackOnBackPressed]
 * runs (the activity finishes).
 */
class OnBackPressedDispatcher(
    private val fallbackOnBackPressed: Runnable? = null,
    private val onHasEnabledCallbacksChanged: ((Boolean) -> Unit)? = null,
) {
    constructor(fallbackOnBackPressed: Runnable?) : this(fallbackOnBackPressed, null)

    private val callbacks = ArrayDeque<OnBackPressedCallback>()
    /** The callback a predictive back gesture started on: it gets the rest of the gesture. */
    private var inProgress: OnBackPressedCallback? = null
    private var hadEnabled = false

    fun addCallback(onBackPressedCallback: OnBackPressedCallback) {
        addCancellableCallback(onBackPressedCallback)
    }

    /** Adds the callback while [owner] is at least started; it is removed when the owner is destroyed. */
    fun addCallback(owner: LifecycleOwner, onBackPressedCallback: OnBackPressedCallback) {
        val lifecycle = owner.lifecycle
        if (lifecycle.currentState == Lifecycle.State.DESTROYED) return
        var current: Cancellable? = null
        val observer = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_START -> if (current == null) current = addCancellableCallback(onBackPressedCallback)
                Lifecycle.Event.ON_STOP -> { current?.cancel(); current = null }
                Lifecycle.Event.ON_DESTROY -> { current?.cancel(); current = null }
                else -> {}
            }
        }
        lifecycle.addObserver(observer)
        onBackPressedCallback.addCancellable(Cancellable {
            lifecycle.removeObserver(observer)
            current?.cancel()
            current = null
        })
    }

    private fun addCancellableCallback(callback: OnBackPressedCallback): Cancellable {
        callbacks.addLast(callback)
        lateinit var c: Cancellable
        c = Cancellable {
            callbacks.remove(callback)
            if (inProgress === callback) {
                callback.handleOnBackCancelled()
                inProgress = null
            }
            callback.removeCancellable(c)
            callback.enabledChangedCallback = null
            updateEnabledCallbacks()
        }
        callback.addCancellable(c)
        callback.enabledChangedCallback = ::updateEnabledCallbacks
        updateEnabledCallbacks()
        return c
    }

    private fun updateEnabledCallbacks() {
        val has = hasEnabledCallbacks()
        if (has != hadEnabled) {
            hadEnabled = has
            onHasEnabledCallbacksChanged?.invoke(has)
        }
    }

    fun hasEnabledCallbacks(): Boolean = callbacks.any { it.isEnabled }

    private fun top(): OnBackPressedCallback? = callbacks.lastOrNull { it.isEnabled }

    fun dispatchOnBackStarted(backEvent: BackEventCompat) {
        val callback = top()
        if (inProgress != null) dispatchOnBackCancelled()
        inProgress = callback
        callback?.handleOnBackStarted(backEvent)
    }

    fun dispatchOnBackProgressed(backEvent: BackEventCompat) {
        (inProgress ?: top())?.handleOnBackProgressed(backEvent)
    }

    fun onBackPressed() {
        val callback = inProgress ?: top()
        inProgress = null
        if (callback != null) callback.handleOnBackPressed() else fallbackOnBackPressed?.run()
    }

    fun dispatchOnBackCancelled() {
        val callback = inProgress ?: top()
        inProgress = null
        callback?.handleOnBackCancelled()
    }
}

/** Adds a callback running [onBackPressed]; returns it so the caller can disable or remove it. */
fun OnBackPressedDispatcher.addCallback(
    owner: LifecycleOwner? = null,
    enabled: Boolean = true,
    onBackPressed: OnBackPressedCallback.() -> Unit,
): OnBackPressedCallback {
    val callback = object : OnBackPressedCallback(enabled) {
        override fun handleOnBackPressed() = onBackPressed()
    }
    if (owner != null) addCallback(owner, callback) else addCallback(callback)
    return callback
}

interface OnBackPressedDispatcherOwner : LifecycleOwner {
    val onBackPressedDispatcher: OnBackPressedDispatcher
}
