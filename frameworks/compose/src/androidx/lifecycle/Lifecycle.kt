package androidx.lifecycle

/** A component's lifecycle (AndroidX `Lifecycle`): its [currentState] and observers of state changes. */
abstract class Lifecycle {
    abstract val currentState: State

    abstract fun addObserver(observer: LifecycleObserver)

    abstract fun removeObserver(observer: LifecycleObserver)

    enum class State {
        DESTROYED, INITIALIZED, CREATED, STARTED, RESUMED;

        fun isAtLeast(state: State): Boolean = compareTo(state) >= 0
    }

    enum class Event {
        ON_CREATE, ON_START, ON_RESUME, ON_PAUSE, ON_STOP, ON_DESTROY, ON_ANY;

        /** The state the lifecycle is in after this event. */
        val targetState: State
            get() = when (this) {
                ON_CREATE, ON_STOP -> State.CREATED
                ON_START, ON_PAUSE -> State.STARTED
                ON_RESUME -> State.RESUMED
                ON_DESTROY -> State.DESTROYED
                ON_ANY -> throw IllegalArgumentException("$this has no target state")
            }

        companion object {
            fun upFrom(state: State): Event? = when (state) {
                State.INITIALIZED -> ON_CREATE
                State.CREATED -> ON_START
                State.STARTED -> ON_RESUME
                else -> null
            }

            fun downFrom(state: State): Event? = when (state) {
                State.CREATED -> ON_DESTROY
                State.STARTED -> ON_STOP
                State.RESUMED -> ON_PAUSE
                else -> null
            }
        }
    }
}

interface LifecycleObserver

fun interface LifecycleEventObserver : LifecycleObserver {
    fun onStateChanged(source: LifecycleOwner, event: Lifecycle.Event)
}

interface LifecycleOwner {
    val lifecycle: Lifecycle
}

/** A [Lifecycle] moved by its owner through [handleLifecycleEvent] / [currentState], one step at a time. */
open class LifecycleRegistry(private val owner: LifecycleOwner) : Lifecycle() {
    private val observers = ArrayList<LifecycleObserver>()
    private var state = State.INITIALIZED

    override var currentState: State
        get() = state
        set(value) = moveTo(value)

    fun handleLifecycleEvent(event: Lifecycle.Event) = moveTo(event.targetState)

    private fun moveTo(target: State) {
        while (state != target) {
            val event = (if (state < target) Event.upFrom(state) else Event.downFrom(state)) ?: break
            state = event.targetState
            for (o in observers.toList()) (o as? LifecycleEventObserver)?.onStateChanged(owner, event)
        }
    }

    override fun addObserver(observer: LifecycleObserver) {
        if (observer in observers) return
        observers.add(observer)
        // A new observer is brought up to the current state, as AndroidX does.
        val e = observer as? LifecycleEventObserver ?: return
        var s = State.INITIALIZED
        while (s < state && state != State.DESTROYED) {
            val up = Event.upFrom(s) ?: break
            s = up.targetState
            e.onStateChanged(owner, up)
        }
    }

    override fun removeObserver(observer: LifecycleObserver) {
        observers.remove(observer)
    }
}
