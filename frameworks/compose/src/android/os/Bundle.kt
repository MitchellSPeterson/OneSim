package android.os

/** A string-keyed map of values (saved instance state, navigation arguments). */
class Bundle() {
    private val map = LinkedHashMap<String, Any?>()

    constructor(other: Bundle) : this() {
        map.putAll(other.map)
    }

    fun putString(key: String, value: String?) { map[key] = value }
    fun getString(key: String): String? = map[key] as? String
    fun getString(key: String, defaultValue: String): String = map[key] as? String ?: defaultValue
    fun putInt(key: String, value: Int) { map[key] = value }
    fun getInt(key: String, defaultValue: Int = 0): Int = map[key] as? Int ?: defaultValue
    fun putLong(key: String, value: Long) { map[key] = value }
    fun getLong(key: String, defaultValue: Long = 0L): Long = map[key] as? Long ?: defaultValue
    fun putFloat(key: String, value: Float) { map[key] = value }
    fun getFloat(key: String, defaultValue: Float = 0f): Float = map[key] as? Float ?: defaultValue
    fun putDouble(key: String, value: Double) { map[key] = value }
    fun getDouble(key: String, defaultValue: Double = 0.0): Double = map[key] as? Double ?: defaultValue
    fun putBoolean(key: String, value: Boolean) { map[key] = value }
    fun getBoolean(key: String, defaultValue: Boolean = false): Boolean = map[key] as? Boolean ?: defaultValue
    fun putStringArray(key: String, value: Array<String>?) { map[key] = value }
    @Suppress("UNCHECKED_CAST")
    fun getStringArray(key: String): Array<String>? = map[key] as? Array<String>
    fun putIntArray(key: String, value: IntArray?) { map[key] = value }
    fun getIntArray(key: String): IntArray? = map[key] as? IntArray
    fun putBundle(key: String, value: Bundle?) { map[key] = value }
    fun getBundle(key: String): Bundle? = map[key] as? Bundle
    fun putAll(other: Bundle) { map.putAll(other.map) }

    @Deprecated("Use the type-specific getters")
    fun get(key: String): Any? = map[key]

    internal fun raw(key: String): Any? = map[key]
    internal fun putRaw(key: String, value: Any?) { map[key] = value }

    fun remove(key: String) { map.remove(key) }
    fun containsKey(key: String) = key in map
    fun keySet(): Set<String> = map.keys
    fun size(): Int = map.size
    fun isEmpty() = map.isEmpty()
    fun clear() = map.clear()

    override fun equals(other: Any?) = other is Bundle && other.map == map
    override fun hashCode() = map.hashCode()
    override fun toString() = "Bundle[$map]"
}

/** `androidx.core.os.bundleOf`. */
fun bundleOf(vararg pairs: Pair<String, Any?>): Bundle = Bundle().also { b -> pairs.forEach { (k, v) -> b.putRaw(k, v) } }
