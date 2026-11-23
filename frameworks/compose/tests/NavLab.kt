// Navigation and system back over the guest wire (frameworks/compose/tests/nav_wire_test.py).
package lab

import androidx.activity.BackEventCompat
import androidx.activity.compose.BackHandler
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.PrimaryScrollableTabRow
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.dialog
import androidx.navigation.compose.navigation
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument
import kotlinx.coroutines.CancellationException

@Composable
private fun Act(label: String, onClick: () -> Unit) = Text(label, Modifier.clickable(onClick = onClick))

@Composable
fun NavLab() {
    val nav = rememberNavController()
    val entry by nav.currentBackStackEntryAsState()
    var log by remember { mutableStateOf("") }
    var handler by remember { mutableStateOf(false) }
    var predictive by remember { mutableStateOf(false) }
    Column {
        Text("route=${entry?.destination?.route}")
        Text("stack=${nav.currentBackStack.value.filter { it.destination.route != null }.joinToString(",") { it.destination.route!! }}")
        Text("log=$log")
        Act("act:detail") { nav.navigate("detail/7") }
        Act("act:detailq") { nav.navigate("detail/8?tab=x%20y") }
        Act("act:single") { nav.navigate("detail/9") { launchSingleTop = true } }
        Act("act:tabB") {
            nav.navigate("b") {
                popUpTo(nav.graph.findStartDestination().id) { saveState = true }
                launchSingleTop = true
                restoreState = true
            }
        }
        Act("act:tabA") {
            nav.navigate("home") {
                popUpTo(nav.graph.findStartDestination().id) { saveState = true }
                launchSingleTop = true
                restoreState = true
            }
        }
        Act("act:nested") { nav.navigate("flow") }
        Act("act:popHome") { log = "pop=" + nav.popBackStack("home", inclusive = false) }
        Act("act:popMissing") { log = "pop=" + nav.popBackStack("nowhere", inclusive = false) }
        Act("act:up") { log = "up=" + nav.navigateUp() }
        Act("act:dialog") { nav.navigate("about") }
        Act("act:bad") {
            log = try {
                nav.navigate("missing/route"); "navigated"
            } catch (e: IllegalArgumentException) {
                "bad"
            }
        }
        Act("act:result") { nav.previousBackStackEntry?.savedStateHandle?.set("result", "done"); nav.popBackStack() }
        Act("act:flowTwo") { nav.navigate("flow/two") }
        Act("act:popAll") { log = "pop=" + nav.popBackStack("home", inclusive = true) + " current=" + nav.currentDestination?.route }
        Act("act:handler") { handler = !handler }
        Act("act:predictive") { predictive = !predictive }
        if (handler) BackHandler { log = "handler" }
        if (predictive) {
            PredictiveBackHandler { progress ->
                var last = -1f
                try {
                    progress.collect { e: BackEventCompat -> last = e.progress; log = "progress=${e.progress} edge=${e.swipeEdge}" }
                    log = "committed after $last"
                } catch (c: CancellationException) {
                    log = "cancelled at $last"
                    throw c
                }
            }
        }
        var tab by remember { mutableIntStateOf(0) }
        PrimaryTabRow(selectedTabIndex = tab) {
            listOf("T0", "T1", "T2").forEachIndexed { i, t -> Tab(selected = tab == i, onClick = { tab = i }, text = { Text(t) }) }
        }
        var stab by remember { mutableIntStateOf(0) }
        PrimaryScrollableTabRow(selectedTabIndex = stab) {
            (0..7).forEach { i -> Tab(selected = stab == i, onClick = { stab = i }, text = { Text("S$i") }) }
        }
        NavHost(nav, startDestination = "home") {
            composable("home") {
                var n by rememberSaveable { mutableIntStateOf(0) }
                val result = it.savedStateHandle.get<String>("result")
                Act("home n=$n result=$result") { n++ }
            }
            composable("detail/{id}?tab={tab}", arguments = listOf(navArgument("id") { type = NavType.IntType }, navArgument("tab") { defaultValue = "main" })) {
                Text("detail id=${it.arguments?.getInt("id")} tab=${it.arguments?.getString("tab")}")
            }
            composable("b") {
                var n by rememberSaveable { mutableIntStateOf(0) }
                Act("b n=$n") { n++ }
            }
            navigation(startDestination = "flow/one", route = "flow") {
                composable("flow/one") { Act("flow one") { nav.navigate("flow/two") } }
                composable("flow/two") { Act("flow two") { nav.popBackStack("flow", inclusive = true) } }
            }
            dialog("about") { Text("about dialog") }
        }
    }
}
