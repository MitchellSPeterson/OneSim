package lab

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Button
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalWideNavigationRail
import androidx.compose.material3.ModalWideNavigationRailProperties
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.ShortNavigationBar
import androidx.compose.material3.ShortNavigationBarItem
import androidx.compose.material3.Text
import androidx.compose.material3.WideNavigationRail
import androidx.compose.material3.WideNavigationRailItem
import androidx.compose.material3.WideNavigationRailValue
import androidx.compose.material3.rememberWideNavigationRailState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch

@Composable
private fun Dot() = Box(Modifier.size(24.dp).background(LocalContentColor.current, CircleShape))

/** Navigation bars and rails driven over the wire by `navbar_wire_test.py`. */
@Composable
fun NavBarLab() = MaterialTheme {
    var sel by remember { mutableIntStateOf(0) }
    val wide = rememberWideNavigationRailState()
    val modal = rememberWideNavigationRailState()
    val locked = rememberWideNavigationRailState()
    val scope = rememberCoroutineScope()
    Column {
        Text("Sel $sel")
        Text("Wide ${wide.currentValue} ${wide.targetValue}")
        Text("Modal ${modal.currentValue} ${modal.targetValue}")
        Text("Locked ${locked.currentValue} ${locked.targetValue}")
        Button(onClick = { scope.launch { wide.toggle() } }) { Text("Toggle") }
        Button(onClick = { scope.launch { modal.expand() } }) { Text("OpenModal") }
        Button(onClick = { scope.launch { locked.expand() } }) { Text("OpenLocked") }
        NavigationBar {
            for (i in 0..2) NavigationBarItem(selected = sel == i, onClick = { sel = i }, icon = { Dot() }, label = { Text("A$i") })
        }
        ShortNavigationBar {
            for (i in 3..4) ShortNavigationBarItem(selected = sel == i, onClick = { sel = i }, icon = { Dot() }, label = { Text("S$i") })
        }
        Row(Modifier.height(400.dp)) {
            NavigationRail {
                NavigationRailItem(selected = sel == 5, onClick = { sel = 5 }, icon = { Dot() }, label = { Text("R5") })
                NavigationRailItem(selected = sel == 6, onClick = { sel = 6 }, icon = { Dot() }, label = { Text("R6") }, enabled = false)
            }
            WideNavigationRail(state = wide) {
                WideNavigationRailItem(selected = sel == 7, onClick = { sel = 7 }, icon = { Dot() }, label = { Text("W7") },
                    railExpanded = wide.targetValue == WideNavigationRailValue.Expanded)
            }
            ModalWideNavigationRail(state = modal) {
                for (i in 8..9) WideNavigationRailItem(selected = sel == i, onClick = { sel = i }, icon = { Dot() }, label = { Text("M$i") },
                    railExpanded = modal.targetValue == WideNavigationRailValue.Expanded)
            }
            ModalWideNavigationRail(state = locked, hideOnCollapse = true, expandedProperties = ModalWideNavigationRailProperties(shouldDismissOnBackPress = false)) {
                WideNavigationRailItem(selected = sel == 10, onClick = { sel = 10 }, icon = { Dot() }, label = { Text("L10") }, railExpanded = true)
            }
        }
    }
}
