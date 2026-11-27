package com.example.navigation

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Button
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.DismissibleDrawerSheet
import androidx.compose.material3.DismissibleNavigationDrawer
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.LargeFlexibleTopAppBar
import androidx.compose.material3.LeadingIconTab
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalWideNavigationRail
import androidx.compose.material3.NavigationDrawerItem
import androidx.compose.material3.NavigationDrawerItemDefaults
import androidx.compose.material3.PermanentDrawerSheet
import androidx.compose.material3.PermanentNavigationDrawer
import androidx.compose.material3.PrimaryScrollableTabRow
import androidx.compose.material3.SecondaryTabRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TwoRowsTopAppBar
import androidx.compose.material3.WideNavigationRailItem
import androidx.compose.material3.rememberDrawerState
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

/** More top app bars and tab rows: centre-aligned, large flexible and two-row bars, leading-icon and scrollable tabs. */
class MoreBarsActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { MoreBarsScreen() }
    }
}

/** A permanent navigation drawer beside the content. */
class PermanentDrawerActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { PermanentDrawerScreen() }
    }
}

/** A dismissible navigation drawer that pushes the content aside. */
class DismissibleDrawerActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { DismissibleDrawerScreen() }
    }
}

/** A modal wide navigation rail expanded over the content. */
class ModalRailActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { ModalRailScreen() }
    }
}

@Composable
fun MoreBarsScreen() = MaterialTheme { MoreBarsContent() }

@OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
@Composable
private fun MoreBarsContent() {
    var tab by remember { mutableIntStateOf(0) }
    var scrolling by remember { mutableIntStateOf(3) }
    Surface(Modifier.fillMaxSize()) {
        Column {
            CenterAlignedTopAppBar(title = { Text("Centered") }, navigationIcon = { GlyphButton() }, actions = { GlyphButton(round = false) })
            LargeFlexibleTopAppBar(title = { Text("Large flexible") }, subtitle = { Text("With a subtitle") }, windowInsets = WindowInsets(0.dp))
            TwoRowsTopAppBar(title = { Text("Two rows") }, navigationIcon = { GlyphButton() }, windowInsets = WindowInsets(0.dp))
            SecondaryTabRow(selectedTabIndex = tab) {
                listOf("Flights", "Hotels").forEachIndexed { i, t ->
                    LeadingIconTab(selected = tab == i, onClick = { tab = i }, text = { Text(t) }, icon = { Glyph(round = i == 0) })
                }
            }
            PrimaryScrollableTabRow(selectedTabIndex = scrolling) {
                (1..8).forEach { i -> Tab(selected = scrolling == i - 1, onClick = { scrolling = i - 1 }, text = { Text("Day $i") }) }
            }
        }
    }
}

@Composable
private fun DrawerItems(selected: Int, onSelect: (Int) -> Unit) {
    Text("Mail", Modifier.padding(16.dp), style = MaterialTheme.typography.titleSmall)
    listOf("Inbox", "Outbox", "Trash").forEachIndexed { i, label ->
        NavigationDrawerItem(
            label = { Text(label) },
            selected = selected == i,
            onClick = { onSelect(i) },
            icon = { Glyph(round = i != 1) },
            modifier = Modifier.padding(NavigationDrawerItemDefaults.ItemPadding),
        )
    }
}

@Composable
fun PermanentDrawerScreen() = MaterialTheme {
    var selected by remember { mutableIntStateOf(0) }
    PermanentNavigationDrawer(drawerContent = { PermanentDrawerSheet(Modifier.width(240.dp)) { DrawerItems(selected) { selected = it } } }) {
        Text("Item ${selected + 1}", Modifier.statusBarsPadding().padding(16.dp), style = MaterialTheme.typography.titleLarge)
    }
}

@Composable
fun DismissibleDrawerScreen() = MaterialTheme {
    val state = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    var selected by remember { mutableIntStateOf(0) }
    DismissibleNavigationDrawer(drawerState = state, drawerContent = { DismissibleDrawerSheet(drawerState = state) { DrawerItems(selected) { selected = it } } }) {
        Surface(Modifier.fillMaxSize()) {
            Column(Modifier.statusBarsPadding().padding(16.dp)) {
                Text("Item ${selected + 1}", style = MaterialTheme.typography.titleLarge)
                Button(onClick = { scope.launch { state.open() } }) { Text("Open") }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun ModalRailScreen() = MaterialTheme {
    val state = rememberWideNavigationRailState()
    val scope = rememberCoroutineScope()
    var selected by remember { mutableIntStateOf(0) }
    Surface(Modifier.fillMaxSize()) {
        Row {
            ModalWideNavigationRail(state = state) {
                listOf("Home", "Search", "Saved").forEachIndexed { i, label ->
                    WideNavigationRailItem(
                        railExpanded = state.targetValue == androidx.compose.material3.WideNavigationRailValue.Expanded,
                        selected = selected == i,
                        onClick = { selected = i },
                        icon = { Glyph(round = i != 1) },
                        label = { Text(label) },
                    )
                }
            }
            Column(Modifier.statusBarsPadding().padding(16.dp)) {
                Text("Item ${selected + 1}", style = MaterialTheme.typography.titleLarge)
                Button(onClick = { scope.launch { state.expand() } }) { Text("Expand rail") }
            }
        }
    }
}
