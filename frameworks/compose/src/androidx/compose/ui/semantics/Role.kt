package androidx.compose.ui.semantics

import androidx.compose.ui.Modifier
import kotlin.jvm.JvmInline

/** Accessibility role of a clickable. */
@JvmInline
value class Role private constructor(internal val wire: String) {
    companion object {
        val Button = Role("button")
        val Checkbox = Role("checkbox")
        val Switch = Role("switch")
        val RadioButton = Role("radio")
        val Tab = Role("tab")
        val Image = Role("image")
        val DropdownList = Role("button")
    }
    override fun toString() = wire
}

/** Accepted for source compatibility; semantics other than the clickable's role and label are not modelled. */
fun Modifier.semantics(mergeDescendants: Boolean = false, properties: SemanticsPropertyReceiver.() -> Unit): Modifier = this

fun Modifier.clearAndSetSemantics(properties: SemanticsPropertyReceiver.() -> Unit): Modifier = this

interface SemanticsPropertyReceiver

var SemanticsPropertyReceiver.contentDescription: String
    get() = ""
    set(@Suppress("UNUSED_PARAMETER") value) {}

var SemanticsPropertyReceiver.role: Role
    get() = Role.Button
    set(@Suppress("UNUSED_PARAMETER") value) {}
