package com.example.navigation

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.NavigationDrawerItem
import androidx.compose.material3.NavigationDrawerItemDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch

/** A modal navigation drawer opened by a button. */
class DrawerActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { DrawerScreen() }
    }
}

@Composable
fun DrawerScreen() = MaterialTheme { DrawerScreenContent() }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DrawerScreenContent() {
    val state = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    var selected by remember { mutableIntStateOf(0) }
    val items = listOf("Inbox", "Outbox", "Favorites", "Trash")
    ModalNavigationDrawer(
        drawerState = state,
        drawerContent = {
            ModalDrawerSheet(drawerState = state) {
                Text("Mail", Modifier.padding(16.dp), style = MaterialTheme.typography.titleSmall)
                HorizontalDivider()
                items.forEachIndexed { i, label ->
                    NavigationDrawerItem(
                        label = { Text(label) },
                        selected = selected == i,
                        onClick = { selected = i; scope.launch { state.close() } },
                        icon = { Glyph(round = i % 2 == 0) },
                        badge = { if (i == 0) Text("24") },
                        modifier = Modifier.padding(NavigationDrawerItemDefaults.ItemPadding),
                    )
                }
            }
        },
    ) {
        Surface(Modifier.fillMaxSize()) {
            Column(Modifier.padding(top = 64.dp, start = 16.dp)) {
                Text("Showing ${items[selected]}", style = MaterialTheme.typography.titleLarge)
                Button(onClick = { scope.launch { state.open() } }) { Text("Open drawer") }
            }
        }
    }
}
