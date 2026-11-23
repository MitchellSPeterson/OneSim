package androidx.navigation

import android.os.Bundle
import androidx.compose.runtime.mutableStateOf
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.SavedStateHandle

/**
 * One destination on the back stack, with its [arguments], a [savedStateHandle] for results, and a lifecycle: RESUMED on
 * top, CREATED below, DESTROYED once popped.
 */
class NavBackStackEntry internal constructor(
    destination: NavDestination,
    arguments: Bundle?,
    val id: String,
) : LifecycleOwner {
    var destination: NavDestination = destination
        internal set

    private val argumentsState = mutableStateOf(arguments)

    /** Arguments as parsed from the route (with defaults). */
    val arguments: Bundle? get() = argumentsState.value?.let { Bundle(it) }

    internal fun replaceArguments(args: Bundle?) {
        argumentsState.value = args
    }

    private val registry = LifecycleRegistry(this)
    override val lifecycle: Lifecycle get() = registry

    internal var maxLifecycle: Lifecycle.State = Lifecycle.State.CREATED
        set(value) {
            field = value
            registry.currentState = value
        }

    val savedStateHandle: SavedStateHandle by lazy {
        val initial = LinkedHashMap<String, Any?>()
        arguments?.let { b -> b.keySet().forEach { k -> initial[k] = b.raw(k) } }
        SavedStateHandle(initial)
    }

    override fun equals(other: Any?) = other is NavBackStackEntry && id == other.id && destination == other.destination
    override fun hashCode() = id.hashCode() * 31 + destination.hashCode()
    override fun toString() = "NavBackStackEntry($id) $destination"
}
