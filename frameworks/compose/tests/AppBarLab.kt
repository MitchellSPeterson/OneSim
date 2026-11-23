package lab

import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.rememberScrollableState
import androidx.compose.foundation.gestures.scrollable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.BottomAppBar
import androidx.compose.material3.BottomAppBarDefaults
import androidx.compose.material3.BottomAppBarState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MediumTopAppBar
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.TopAppBarState
import androidx.compose.material3.rememberBottomAppBarState
import androidx.compose.material3.rememberTopAppBarState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.SaverScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt

private fun Float.r() = if (this == -Float.MAX_VALUE) "min" else roundToInt().toString()
private fun Float.f2() = ((this * 100).roundToInt() / 100.0).toString()

private val scope = object : SaverScope {
    override fun canBeSaved(value: Any) = true
}

@Suppress("UNCHECKED_CAST")
private fun <T : Any> roundTrip(saver: Saver<T, *>, value: T): Pair<Any, T> {
    val s = saver as Saver<T, Any>
    val saved = with(s) { scope.save(value) }!!
    return saved to s.restore(saved)!!
}

/**
 * App bar scroll behaviours checked over the wire by tests/appbar_wire_test.py: one bar per mode (a "Mode" tap cycles),
 * a 2000 px tall scrollable list under it, and a status line with the bar state (pixels).
 */
@Composable
fun AppBarLab() {
    val modes = listOf("pinned", "enter", "exit", "medium", "bottom", "nosnap")
    var mode by remember { mutableStateOf(0) }
    var saved by remember { mutableStateOf("-") }
    MaterialTheme {
        key(mode) {
            val name = modes[mode]
            val top = rememberTopAppBarState()
            val bottom = rememberBottomAppBarState()
            val behavior = when (name) {
                "pinned" -> TopAppBarDefaults.pinnedScrollBehavior(top)
                "enter" -> TopAppBarDefaults.enterAlwaysScrollBehavior(top)
                "nosnap" -> TopAppBarDefaults.enterAlwaysScrollBehavior(top, snapAnimationSpec = null, flingAnimationSpec = null)
                else -> TopAppBarDefaults.exitUntilCollapsedScrollBehavior(top)
            }
            val bottomBehavior = BottomAppBarDefaults.exitAlwaysScrollBehavior(bottom)
            val connection = if (name == "bottom") bottomBehavior.nestedScrollConnection else behavior.nestedScrollConnection
            var scroll by remember { mutableFloatStateOf(0f) }
            val list = rememberScrollableState { delta ->
                val next = (scroll - delta).coerceIn(0f, 2000f)
                val used = scroll - next
                scroll = next
                used
            }
            Column(Modifier.fillMaxSize()) {
                Box(Modifier.clickable { mode = (mode + 1) % modes.size }) { Text("Mode $name") }
                Box(Modifier.clickable {
                    val (s1, t) = roundTrip(TopAppBarState.Saver, top)
                    val (s2, b) = roundTrip(BottomAppBarState.Saver, bottom)
                    saved = "$s1 ${t.heightOffset.r()} ${t.contentOffset.r()} $s2 ${b.heightOffset.r()}"
                }) { Text("Save $saved") }
                Text("Top h=${top.heightOffset.r()} lim=${top.heightOffsetLimit.r()} c=${top.contentOffset.r()} f=${top.collapsedFraction.f2()} o=${top.overlappedFraction.f2()}")
                Text("Bottom h=${bottom.heightOffset.r()} lim=${bottom.heightOffsetLimit.r()} c=${bottom.contentOffset.r()} f=${bottom.collapsedFraction.f2()}")
                Text("List ${scroll.r()}")
                when (name) {
                    "medium" -> MediumTopAppBar(title = { Text("Medium") }, scrollBehavior = behavior, windowInsets = WindowInsets(0.dp))
                    "bottom" -> {}
                    else -> TopAppBar(title = { Text("Bar") }, scrollBehavior = behavior, windowInsets = WindowInsets(0.dp))
                }
                Box(Modifier.weight(1f).fillMaxWidth().nestedScroll(connection).clip(androidx.compose.ui.graphics.RectangleShape)) {
                    Column(Modifier.fillMaxWidth().scrollable(list, Orientation.Vertical).offset { IntOffset(0, -scroll.roundToInt()) }) {
                        for (i in 0 until 40) Row(Modifier.height(60.dp).padding(8.dp)) { Text("Item $i") }
                    }
                }
                if (name == "bottom") {
                    BottomAppBar(actions = { Box(Modifier.size(48.dp)) }, scrollBehavior = bottomBehavior, windowInsets = WindowInsets(0.dp))
                }
            }
        }
    }
}
