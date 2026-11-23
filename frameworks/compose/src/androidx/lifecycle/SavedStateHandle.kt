package androidx.lifecycle

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** Key-value state of a navigation back stack entry (or a ViewModel), kept while the entry is on the back stack. */
class SavedStateHandle(initialState: Map<String, Any?> = emptyMap()) {
    private val values = LinkedHashMap<String, Any?>(initialState)
    private val flows = HashMap<String, MutableStateFlow<Any?>>()

    operator fun contains(key: String): Boolean = key in values

    @Suppress("UNCHECKED_CAST")
    operator fun <T> get(key: String): T? = values[key] as T?

    operator fun <T> set(key: String, value: T?) {
        values[key] = value
        flows[key]?.value = value
    }

    @Suppress("UNCHECKED_CAST")
    fun <T> getStateFlow(key: String, initialValue: T): StateFlow<T> {
        if (key !in values) values[key] = initialValue
        val flow = flows.getOrPut(key) { MutableStateFlow(values[key]) }
        return (flow as MutableStateFlow<T>).asStateFlow()
    }

    fun keys(): Set<String> = values.keys.toSet()

    fun <T> remove(key: String): T? {
        @Suppress("UNCHECKED_CAST")
        val v = values.remove(key) as T?
        flows.remove(key)
        return v
    }
}
