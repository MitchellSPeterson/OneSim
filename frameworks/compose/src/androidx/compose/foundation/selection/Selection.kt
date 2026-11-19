package androidx.compose.foundation.selection

import androidx.compose.ui.ClickModifier
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role

fun Modifier.toggleable(value: Boolean, enabled: Boolean = true, role: Role? = null, onValueChange: (Boolean) -> Unit): Modifier =
    this then ClickModifier(enabled, (role ?: Role.Switch).toString(), null) { onValueChange(!value) }

fun Modifier.selectable(selected: Boolean, enabled: Boolean = true, role: Role? = null, onClick: () -> Unit): Modifier =
    this then ClickModifier(enabled, (role ?: Role.RadioButton).toString(), null, onClick)
