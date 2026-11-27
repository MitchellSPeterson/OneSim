package lab

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.DismissibleDrawerSheet
import androidx.compose.material3.DismissibleNavigationDrawer
import androidx.compose.material3.DrawerState
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.NavigationDrawerItem
import androidx.compose.material3.Text
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch

/** Drawer state probe for drawer_wire_test.py: one line of DrawerState values plus buttons driving it. */
@Composable
fun StateLine(name: String, s: DrawerState) {
    val o = s.currentOffset
    Text("$name cur=${s.currentValue} target=${s.targetValue} open=${s.isOpen} closed=${s.isClosed} anim=${s.isAnimationRunning} off=${if (o.isNaN()) "NaN" else o.toInt().toString()}")
}

@Composable
fun DrawerLab() {
    MaterialTheme {
        var allowClose by remember { mutableStateOf(true) }
        var dismissible by remember { mutableStateOf(false) }
        var picks by remember { mutableIntStateOf(0) }
        val modal = rememberDrawerState(DrawerValue.Closed) { it != DrawerValue.Closed || allowClose }
        val dis = rememberDrawerState(DrawerValue.Closed)
        val scope = rememberCoroutineScope()
        val content: @Composable () -> Unit = {
            Column(Modifier.fillMaxSize().padding(top = 60.dp, start = 30.dp)) {
                StateLine("Modal", modal)
                StateLine("Dis", dis)
                Text("Allow close $allowClose picks $picks")
                Button(onClick = { scope.launch { modal.open() } }) { Text("Open") }
                Button(onClick = { scope.launch { modal.close() } }) { Text("Close") }
                Button(onClick = { scope.launch { modal.snapTo(DrawerValue.Open) } }) { Text("SnapOpen") }
                Button(onClick = { scope.launch { modal.snapTo(DrawerValue.Closed) } }) { Text("SnapClosed") }
                Button(onClick = { allowClose = !allowClose }) { Text("Veto") }
                Button(onClick = { dismissible = !dismissible }) { Text("Mode") }
                Button(onClick = { scope.launch { dis.open() } }) { Text("DisOpen") }
            }
        }
        if (!dismissible) {
            ModalNavigationDrawer(
                drawerState = modal,
                drawerContent = {
                    ModalDrawerSheet(modal) {
                        NavigationDrawerItem(label = { Text("Pick") }, selected = true, onClick = { picks++ })
                    }
                },
                content = content,
            )
        } else {
            DismissibleNavigationDrawer(
                drawerState = dis,
                drawerContent = { DismissibleDrawerSheet(dis) { Text("Dis sheet") } },
                content = content,
            )
        }
    }
}
