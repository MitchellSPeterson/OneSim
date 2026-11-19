package android.os

/** Saved instance state; always empty on first launch. */
class Bundle {
    private val map = mutableMapOf<String, Any?>()
    fun putString(key: String, value: String?) { map[key] = value }
    fun getString(key: String): String? = map[key] as String?
    fun putInt(key: String, value: Int) { map[key] = value }
    fun getInt(key: String, defaultValue: Int = 0): Int = map[key] as Int? ?: defaultValue
    fun putBoolean(key: String, value: Boolean) { map[key] = value }
    fun getBoolean(key: String, defaultValue: Boolean = false): Boolean = map[key] as Boolean? ?: defaultValue
    fun containsKey(key: String) = key in map
    fun isEmpty() = map.isEmpty()
}
