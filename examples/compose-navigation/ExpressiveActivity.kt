package com.example.navigation

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FlexibleBottomAppBar
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MediumFlexibleTopAppBar
import androidx.compose.material3.Scaffold
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/** Material 3 Expressive navigation: a flexible top app bar, an expanded wide rail, a short bar and a flexible bottom bar. */
class ExpressiveActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { ExpressiveScreen() }
    }
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun ExpressiveScreen() = MaterialTheme { ExpressiveScreenContent() }

@OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
@Composable
private fun ExpressiveScreenContent() {
    var bar by remember { mutableIntStateOf(0) }
    var rail by remember { mutableIntStateOf(0) }
    val railState = rememberWideNavigationRailState(WideNavigationRailValue.Expanded)
    Scaffold(
        topBar = {
            MediumFlexibleTopAppBar(
                title = { Text("Photos") },
                subtitle = { Text("12 albums") },
                navigationIcon = { GlyphButton() },
            )
        },
        bottomBar = {
            Column {
                FlexibleBottomAppBar { GlyphButton(); GlyphButton(round = false); GlyphButton() }
                ShortNavigationBar {
                    listOf("For you", "Albums", "Shared").forEachIndexed { i, label ->
                        ShortNavigationBarItem(selected = bar == i, onClick = { bar = i }, icon = { Glyph(round = i != 1) }, label = { Text(label) })
                    }
                }
            }
        },
    ) { padding ->
        Row(Modifier.padding(padding).fillMaxSize()) {
            WideNavigationRail(state = railState) {
                listOf("Recent", "Starred").forEachIndexed { i, label ->
                    WideNavigationRailItem(
                        railExpanded = true,
                        selected = rail == i,
                        onClick = { rail = i },
                        icon = { Glyph(round = i == 0) },
                        label = { Text(label) },
                    )
                }
            }
            Text("Section ${rail + 1}", Modifier.padding(16.dp), style = MaterialTheme.typography.titleMedium)
        }
    }
}
