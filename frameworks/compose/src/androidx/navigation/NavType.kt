package androidx.navigation

import android.os.Bundle
import kotlin.jvm.JvmField
import kotlin.jvm.JvmStatic

/** How an argument's value is stored in a [Bundle] and parsed from a route. */
abstract class NavType<T>(open val isNullableAllowed: Boolean) {
    abstract fun put(bundle: Bundle, key: String, value: T)
    abstract operator fun get(bundle: Bundle, key: String): T?
    abstract fun parseValue(value: String): T
    open fun parseValue(value: String, previousValue: T): T = parseValue(value)
    open fun serializeAsValue(value: T): String = value.toString()
    open val name: String = "nav_type"

    internal fun parseAndPut(bundle: Bundle, key: String, value: String): T {
        val parsed = parseValue(value)
        put(bundle, key, parsed)
        return parsed
    }

    override fun toString() = name

    companion object {
        @JvmField val IntType: NavType<Int> = object : NavType<Int>(false) {
            override val name = "integer"
            override fun put(bundle: Bundle, key: String, value: Int) = bundle.putInt(key, value)
            override fun get(bundle: Bundle, key: String): Int? = bundle.raw(key) as? Int
            override fun parseValue(value: String): Int =
                if (value.startsWith("0x")) value.substring(2).toInt(16) else value.toInt()
        }
        @JvmField val LongType: NavType<Long> = object : NavType<Long>(false) {
            override val name = "long"
            override fun put(bundle: Bundle, key: String, value: Long) = bundle.putLong(key, value)
            override fun get(bundle: Bundle, key: String): Long? = bundle.raw(key) as? Long
            override fun parseValue(value: String): Long {
                val v = if (value.endsWith("L")) value.dropLast(1) else value
                return if (v.startsWith("0x")) v.substring(2).toLong(16) else v.toLong()
            }
        }
        @JvmField val FloatType: NavType<Float> = object : NavType<Float>(false) {
            override val name = "float"
            override fun put(bundle: Bundle, key: String, value: Float) = bundle.putFloat(key, value)
            override fun get(bundle: Bundle, key: String): Float? = bundle.raw(key) as? Float
            override fun parseValue(value: String): Float = value.toFloat()
        }
        @JvmField val BoolType: NavType<Boolean> = object : NavType<Boolean>(false) {
            override val name = "boolean"
            override fun put(bundle: Bundle, key: String, value: Boolean) = bundle.putBoolean(key, value)
            override fun get(bundle: Bundle, key: String): Boolean? = bundle.raw(key) as? Boolean
            override fun parseValue(value: String): Boolean = when (value) {
                "true" -> true
                "false" -> false
                else -> throw IllegalArgumentException("A boolean NavType only accepts \"true\" or \"false\" values.")
            }
        }
        @JvmField val StringType: NavType<String?> = object : NavType<String?>(true) {
            override val name = "string"
            override fun put(bundle: Bundle, key: String, value: String?) = bundle.putString(key, value)
            override fun get(bundle: Bundle, key: String): String? = bundle.getString(key)
            override fun parseValue(value: String): String? = if (value == "null") null else value
            override fun serializeAsValue(value: String?): String = value ?: "null"
        }
        @JvmField val IntArrayType: NavType<IntArray?> = object : NavType<IntArray?>(true) {
            override val name = "integer[]"
            override fun put(bundle: Bundle, key: String, value: IntArray?) = bundle.putIntArray(key, value)
            override fun get(bundle: Bundle, key: String): IntArray? = bundle.getIntArray(key)
            override fun parseValue(value: String): IntArray = intArrayOf(IntType.parseValue(value))
            override fun parseValue(value: String, previousValue: IntArray?): IntArray = (previousValue ?: intArrayOf()) + parseValue(value)
        }
        @JvmField val StringArrayType: NavType<Array<String>?> = object : NavType<Array<String>?>(true) {
            override val name = "string[]"
            override fun put(bundle: Bundle, key: String, value: Array<String>?) = bundle.putStringArray(key, value)
            override fun get(bundle: Bundle, key: String): Array<String>? = bundle.getStringArray(key)
            override fun parseValue(value: String): Array<String> = arrayOf(value)
            override fun parseValue(value: String, previousValue: Array<String>?): Array<String> = (previousValue ?: arrayOf()) + value
        }

        /** The type named in an XML graph's `argType` (`integer`, `long`, `float`, `boolean`, `string`...). */
        @JvmStatic
        fun fromArgType(type: String?, packageName: String?): NavType<*> = when (type) {
            "integer" -> IntType
            "long" -> LongType
            "float" -> FloatType
            "boolean" -> BoolType
            "integer[]" -> IntArrayType
            "string[]" -> StringArrayType
            null, "string" -> StringType
            else -> throw IllegalArgumentException("Object of type $type is not supported for navigation arguments.")
        }
    }
}

/** An argument of a destination: its [type], nullability and default. */
class NavArgument internal constructor(
    val type: NavType<Any?>,
    val isNullable: Boolean,
    private val defaultValueValue: Any?,
    val isDefaultValuePresent: Boolean,
) {
    val defaultValue: Any? get() = defaultValueValue

    internal fun putDefaultValue(name: String, bundle: Bundle) {
        if (isDefaultValuePresent) type.put(bundle, name, defaultValueValue)
    }

    class Builder {
        private var type: NavType<Any?>? = null
        private var isNullable = false
        private var defaultValue: Any? = null
        private var defaultValuePresent = false

        @Suppress("UNCHECKED_CAST")
        fun <T> setType(type: NavType<T>): Builder = apply { this.type = type as NavType<Any?> }
        fun setIsNullable(isNullable: Boolean): Builder = apply { this.isNullable = isNullable }
        fun setDefaultValue(defaultValue: Any?): Builder = apply {
            this.defaultValue = defaultValue
            defaultValuePresent = true
        }

        fun build(): NavArgument {
            @Suppress("UNCHECKED_CAST")
            val t = type ?: (inferType(defaultValue) as NavType<Any?>)
            require(!isNullable || t.isNullableAllowed) { "${t.name} does not allow nullable values" }
            require(isNullable || !defaultValuePresent || defaultValue != null) { "Argument with type ${t.name} has null value but is not nullable." }
            return NavArgument(t, isNullable, defaultValue, defaultValuePresent)
        }

        private fun inferType(v: Any?): NavType<*> = when (v) {
            is Int -> NavType.IntType
            is Long -> NavType.LongType
            is Float -> NavType.FloatType
            is Boolean -> NavType.BoolType
            is IntArray -> NavType.IntArrayType
            else -> NavType.StringType
        }
    }
}

class NavArgumentBuilder {
    private val builder = NavArgument.Builder()
    private var typeSet = false

    var type: NavType<*> = NavType.StringType
        set(value) {
            typeSet = true
            field = value
            builder.setType(value)
        }
    var nullable: Boolean = false
        set(value) {
            field = value
            builder.setIsNullable(value)
        }
    var defaultValue: Any? = null
        set(value) {
            field = value
            builder.setDefaultValue(value)
        }

    fun unknownDefaultValuePresent() {}

    fun build(): NavArgument = builder.build()
}

/** An argument [name] and its [argument]. */
class NamedNavArgument internal constructor(val name: String, val argument: NavArgument) {
    operator fun component1(): String = name
    operator fun component2(): NavArgument = argument
}

fun navArgument(name: String, builder: NavArgumentBuilder.() -> Unit): NamedNavArgument =
    NamedNavArgument(name, NavArgumentBuilder().apply(builder).build())

/** A deep link pattern of a destination. OneSim stores them; the system never opens one. */
class NavDeepLink internal constructor(val uriPattern: String?, val action: String?, val mimeType: String?) {
    class Builder {
        private var uri: String? = null
        private var action: String? = null
        private var mime: String? = null
        fun setUriPattern(uriPattern: String) = apply { uri = uriPattern }
        fun setAction(action: String) = apply { this.action = action }
        fun setMimeType(mimeType: String) = apply { mime = mimeType }
        fun build() = NavDeepLink(uri, action, mime)

        companion object {
            @JvmStatic fun fromUriPattern(uriPattern: String) = Builder().setUriPattern(uriPattern)
            @JvmStatic fun fromAction(action: String) = Builder().setAction(action)
            @JvmStatic fun fromMimeType(mimeType: String) = Builder().setMimeType(mimeType)
        }
    }
}

class NavDeepLinkDslBuilder {
    var uriPattern: String? = null
    var action: String? = null
    var mimeType: String? = null
    internal fun build() = NavDeepLink(uriPattern, action, mimeType)
}

fun navDeepLink(deepLinkBuilder: NavDeepLinkDslBuilder.() -> Unit): NavDeepLink = NavDeepLinkDslBuilder().apply(deepLinkBuilder).build()
