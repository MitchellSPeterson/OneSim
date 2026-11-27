package com.example.navigation

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MediumTopAppBar
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument

/** Scaffold with a medium top app bar, primary tabs, a NavHost and a navigation bar. */
class NavigationActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { NavigationScreen() }
    }
}

private val sections = listOf("inbox" to "Inbox", "search" to "Search", "people" to "People", "settings" to "Settings")

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NavigationScreen() = MaterialTheme { NavigationScreenContent() }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun NavigationScreenContent() {
    val nav = rememberNavController()
    val entry by nav.currentBackStackEntryAsState()
    val route = entry?.destination?.route ?: "inbox"
    Scaffold(
        topBar = {
            MediumTopAppBar(
                title = { Text(sections.firstOrNull { route.startsWith(it.first) }?.second ?: "Message") },
                navigationIcon = { GlyphButton() },
                actions = { GlyphButton(round = false); GlyphButton() },
            )
        },
        bottomBar = {
            NavigationBar {
                sections.forEach { (r, label) ->
                    NavigationBarItem(
                        selected = route == r,
                        onClick = {
                            nav.navigate(r) {
                                popUpTo(nav.graph.findStartDestination().id) { saveState = true }
                                launchSingleTop = true
                                restoreState = true
                            }
                        },
                        icon = { Glyph(round = r != "search") },
                        label = { Text(label) },
                    )
                }
            }
        },
    ) { padding ->
        NavHost(nav, startDestination = "inbox", modifier = Modifier.padding(padding)) {
            composable("inbox") { Inbox(onOpen = { nav.navigate("message/$it") }) }
            composable("message/{id}", arguments = listOf(navArgument("id") { type = NavType.IntType })) {
                Text("Message ${it.arguments?.getInt("id")}", Modifier.padding(16.dp), style = MaterialTheme.typography.titleMedium)
            }
            composable("search") { Text("Search everything", Modifier.padding(16.dp)) }
            composable("people") { Text("People you know", Modifier.padding(16.dp)) }
            composable("settings") { Text("Settings", Modifier.padding(16.dp)) }
        }
    }
}

@Composable
private fun Inbox(onOpen: (Int) -> Unit) {
    var tab by remember { mutableIntStateOf(0) }
    val tabs = listOf("Primary", "Social", "Updates")
    Column(Modifier.fillMaxSize()) {
        PrimaryTabRow(selectedTabIndex = tab) {
            tabs.forEachIndexed { i, t -> Tab(selected = tab == i, onClick = { tab = i }, text = { Text(t) }) }
        }
        (1..3).forEach { i ->
            Text(
                "${tabs[tab]} message $i",
                Modifier.fillMaxWidth().clickable { onOpen(i) }.padding(horizontal = 16.dp, vertical = 14.dp),
                style = MaterialTheme.typography.bodyLarge,
            )
        }
    }
}
