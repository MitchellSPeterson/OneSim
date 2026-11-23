package com.example.interaction

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.draggable2D
import androidx.compose.foundation.gestures.rememberDraggable2DState
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.gestures.rememberScrollableState
import androidx.compose.foundation.gestures.scrollable
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.round
import kotlin.math.roundToInt

class InteractionActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { MaterialTheme { Interaction() } }
    }
}

private val Track = Color(0xFFF1F0F5)
private val Accent = Color(0xFF6750A4)
private val Soft = Color(0xFFE8DEF8)

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun Interaction() {
    var clicks by remember { mutableIntStateOf(0) }
    var taps by remember { mutableIntStateOf(0) }
    var longs by remember { mutableIntStateOf(0) }
    var doubles by remember { mutableIntStateOf(0) }
    var dragX by remember { mutableFloatStateOf(0f) }
    var drag2 by remember { mutableStateOf(Offset.Zero) }
    var tapAt by remember { mutableStateOf("none") }
    var scrolled by remember { mutableFloatStateOf(0f) }
    var option by remember { mutableIntStateOf(0) }
    var wifi by remember { mutableStateOf(true) }
    val cardSource = remember { MutableInteractionSource() }
    val pressed by cardSource.collectIsPressedAsState()
    val hoverSource = remember { MutableInteractionSource() }
    val hovered by hoverSource.collectIsHoveredAsState()
    val density = LocalDensity.current
    val trackPx = with(density) { 200.dp.toPx() }
    val areaX = with(density) { 200.dp.toPx() }
    val areaY = with(density) { 56.dp.toPx() }
    val scrollMax = with(density) { 120.dp.toPx() }

    Column(Modifier.padding(start = 24.dp, top = 104.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text("Compose interaction")
        Card(onClick = { clicks++ }, modifier = Modifier.width(240.dp), interactionSource = cardSource) {
            Text("Clicks $clicks", Modifier.padding(16.dp))
        }
        Text("Pressed $pressed, hovered $hovered", Modifier.hoverable(hoverSource))
        Box(
            Modifier.width(240.dp).height(48.dp).background(Soft)
                .combinedClickable(onLongClick = { longs++ }, onDoubleClick = { doubles++ }) { taps++ },
            contentAlignment = Alignment.CenterStart,
        ) {
            Text("Tap $taps long $longs double $doubles", Modifier.padding(start = 16.dp))
        }
        Box(Modifier.width(240.dp).height(40.dp).background(Track)) {
            Box(
                Modifier.offset { IntOffset(dragX.roundToInt(), 0) }.size(40.dp).background(Accent)
                    .draggable(rememberDraggableState { dragX = (dragX + it).coerceIn(0f, trackPx) }, Orientation.Horizontal),
            )
        }
        Box(Modifier.width(240.dp).height(96.dp).background(Track)) {
            Box(
                Modifier.offset { drag2.round() }.size(40.dp).background(Color(0xFF7D5260))
                    .draggable2D(rememberDraggable2DState { drag2 = Offset((drag2.x + it.x).coerceIn(0f, areaX), (drag2.y + it.y).coerceIn(0f, areaY)) }),
            )
        }
        Text("Drag ${dragX.roundToInt()} / ${drag2.x.roundToInt()}, ${drag2.y.roundToInt()}")
        Box(
            Modifier.width(240.dp).height(48.dp).background(Color(0xFFFFD8E4))
                .pointerInput(Unit) { detectTapGestures { tapAt = "${it.x.roundToInt()}, ${it.y.roundToInt()}" } },
            contentAlignment = Alignment.CenterStart,
        ) {
            Text("Tap at $tapAt", Modifier.padding(start = 16.dp))
        }
        Box(
            Modifier.width(240.dp).height(48.dp).background(Track)
                .scrollable(
                    rememberScrollableState { d ->
                        val before = scrolled
                        scrolled = (scrolled + d).coerceIn(0f, scrollMax)
                        scrolled - before
                    },
                    Orientation.Horizontal,
                ),
            contentAlignment = Alignment.CenterStart,
        ) {
            Text("Scroll ${scrolled.roundToInt()}", Modifier.padding(start = 16.dp))
        }
        Row(Modifier.selectableGroup(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf("One", "Two", "Three").forEachIndexed { i, label ->
                Box(
                    Modifier.size(72.dp, 40.dp).background(if (option == i) Accent else Soft)
                        .selectable(selected = option == i, onClick = { option = i }, role = Role.RadioButton),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(label, color = if (option == i) Color.White else Color.Black)
                }
            }
        }
        Row(
            Modifier.width(240.dp).toggleable(value = wifi, onValueChange = { wifi = it }, role = Role.Switch),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Wi-Fi")
            Switch(checked = wifi, onCheckedChange = null)
        }
    }
}
