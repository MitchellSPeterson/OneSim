package com.example.navigation

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.BottomAppBar
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.LargeTopAppBar
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SecondaryScrollableTabRow
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.rememberTopAppBarState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/** A large top app bar that collapses, a scrollable secondary tab row, a navigation rail and a bottom app bar. */
class BarsActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { BarsScreen() }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun BarsScreen() = MaterialTheme { BarsScreenContent() }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun BarsScreenContent() {
    val barState = rememberTopAppBarState()
    val scroll = TopAppBarDefaults.exitUntilCollapsedScrollBehavior(barState)
    var tab by remember { mutableIntStateOf(0) }
    var rail by remember { mutableIntStateOf(0) }
    Scaffold(
        topBar = {
            LargeTopAppBar(
                title = { Text("Library") },
                navigationIcon = { GlyphButton() },
                actions = { GlyphButton(round = false) },
                scrollBehavior = scroll,
            )
        },
        bottomBar = {
            BottomAppBar(
                actions = { GlyphButton(); GlyphButton(round = false); GlyphButton() },
                floatingActionButton = { FloatingActionButton(onClick = {}) { Glyph(round = false) } },
            )
        },
    ) { padding ->
        Column(Modifier.padding(padding).fillMaxSize()) {
            SecondaryScrollableTabRow(selectedTabIndex = tab) {
                listOf("Albums", "Artists", "Playlists", "Podcasts", "Downloads", "Recent").forEachIndexed { i, t ->
                    Tab(selected = tab == i, onClick = { tab = i }, text = { Text(t) })
                }
            }
            Row {
                NavigationRail(header = { FloatingActionButton(onClick = {}) { Glyph() } }) {
                    listOf("Home", "Saved", "Share").forEachIndexed { i, label ->
                        NavigationRailItem(selected = rail == i, onClick = { rail = i }, icon = { Glyph(round = i != 1) }, label = { Text(label) })
                    }
                }
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("Rail item ${rail + 1}", style = MaterialTheme.typography.titleMedium)
                    Button(onClick = { barState.heightOffset = barState.heightOffsetLimit }) { Text("Collapse") }
                    Button(onClick = { barState.heightOffset = 0f }) { Text("Expand") }
                }
            }
        }
    }
}
