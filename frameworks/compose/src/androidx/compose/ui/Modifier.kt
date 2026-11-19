package androidx.compose.ui

import androidx.compose.runtime.Stable

/**
 * An ordered chain of modifier elements; the first element is the outermost (protocol section K).
 * OneSim elements are wire entries the host lays out, not node delegates.
 */
@Stable
interface Modifier {
    fun <R> foldIn(initial: R, operation: (R, Element) -> R): R
    fun all(predicate: (Element) -> Boolean): Boolean
    fun any(predicate: (Element) -> Boolean): Boolean
    infix fun then(other: Modifier): Modifier = if (other === Modifier) this else CombinedModifier(this, other)

    interface Element : Modifier {
        override fun <R> foldIn(initial: R, operation: (R, Element) -> R): R = operation(initial, this)
        override fun all(predicate: (Element) -> Boolean) = predicate(this)
        override fun any(predicate: (Element) -> Boolean) = predicate(this)
    }

    companion object : Modifier {
        override fun <R> foldIn(initial: R, operation: (R, Element) -> R): R = initial
        override fun all(predicate: (Element) -> Boolean) = true
        override fun any(predicate: (Element) -> Boolean) = false
        override infix fun then(other: Modifier): Modifier = other
        override fun toString() = "Modifier"
    }
}

class CombinedModifier(internal val outer: Modifier, internal val inner: Modifier) : Modifier {
    override fun <R> foldIn(initial: R, operation: (R, Modifier.Element) -> R): R = inner.foldIn(outer.foldIn(initial, operation), operation)
    override fun all(predicate: (Modifier.Element) -> Boolean) = outer.all(predicate) && inner.all(predicate)
    override fun any(predicate: (Modifier.Element) -> Boolean) = outer.any(predicate) || inner.any(predicate)
    override fun equals(other: Any?) = other is CombinedModifier && outer == other.outer && inner == other.inner
    override fun hashCode() = outer.hashCode() + 31 * inner.hashCode()
}

/** A wire entry (JSON object text). */
data class WireModifier(val json: String) : Modifier.Element

/** A tap target; `onClick` becomes an action id when the tree is sent. */
class ClickModifier(val enabled: Boolean, val role: String?, val label: String?, val onClick: () -> Unit) : Modifier.Element

fun Modifier.wire(json: String): Modifier = this then WireModifier(json)

/** Escape-and-quote a string for JSON. */
fun jsonString(s: String): String = buildString {
    append('"')
    for (ch in s) when {
        ch == '"' -> append("\\\"")
        ch == '\\' -> append("\\\\")
        ch == '\n' -> append("\\n")
        ch == '\r' -> append("\\r")
        ch == '\t' -> append("\\t")
        ch < ' ' -> append("\\u%04x".format(ch.code))
        else -> append(ch)
    }
    append('"')
}

/** JSON number; NaN/infinity (unspecified) become null. */
fun jsonNum(v: Float): String = if (v.isNaN() || v.isInfinite()) "null" else {
    val r = Math.round(v * 1000.0) / 1000.0
    if (r == Math.floor(r) && Math.abs(r) < 1e15) r.toLong().toString() else r.toString()
}
