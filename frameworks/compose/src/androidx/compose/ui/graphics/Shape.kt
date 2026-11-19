package androidx.compose.ui.graphics

import androidx.compose.runtime.Immutable

/** An outline. OneSim shapes describe themselves for the wire (protocol section K). */
@Immutable
interface Shape {
    /** Wire JSON (`{"corners":[...]}` / `{"percent":[...]}`), or null for a rectangle. */
    fun wire(): String?
}

val RectangleShape: Shape = object : Shape {
    override fun wire(): String? = null
    override fun toString() = "RectangleShape"
}
