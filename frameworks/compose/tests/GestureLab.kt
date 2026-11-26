package lab

import androidx.compose.foundation.MutatePriority
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.gestures.rememberScrollableState
import androidx.compose.foundation.gestures.scrollable
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.indication
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.foundation.interaction.collectIsDraggedAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.Text
import androidx.compose.material3.ripple
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt

/** Pointer input behaviour checked over the wire by tests/wire_test.py (no visible reference needed). */
@Composable
fun GestureLab() {
    Column {
        // Nested clickables: the inner one consumes, so the outer one does not click.
        var outer by remember { mutableIntStateOf(0) }
        var inner by remember { mutableIntStateOf(0) }
        Box(Modifier.size(200.dp, 40.dp).clickable { outer++ }) {
            Box(Modifier.size(100.dp, 40.dp).clickable { inner++ }) { Text("Nest $inner $outer") }
        }
        // Pass order along the hit path.
        var log by remember { mutableStateOf("") }
        Box(Modifier.size(200.dp, 40.dp).pointerInput(Unit) {
            awaitPointerEventScope {
                while (true) {
                    awaitPointerEvent(PointerEventPass.Initial); log += "oI "
                    awaitPointerEvent(PointerEventPass.Main); log += "oM "
                    awaitPointerEvent(PointerEventPass.Final); log += "oF "
                }
            }
        }) {
            Box(Modifier.size(100.dp, 40.dp).pointerInput(Unit) {
                awaitPointerEventScope {
                    while (true) {
                        awaitPointerEvent(PointerEventPass.Initial); log += "iI "
                        awaitPointerEvent(PointerEventPass.Main); log += "iM "
                        awaitPointerEvent(PointerEventPass.Final); log += "iF "
                    }
                }
            }) { Text("Log $log") }
        }
        // Long and double clicks; single clicks wait for the double tap timeout.
        var taps by remember { mutableIntStateOf(0) }
        var longs by remember { mutableIntStateOf(0) }
        var doubles by remember { mutableIntStateOf(0) }
        Box(Modifier.size(200.dp, 40.dp).combinedClickable(onLongClick = { longs++ }, onDoubleClick = { doubles++ }) { taps++ }) {
            Text("Combo $taps $longs $doubles")
        }
        // Draggable callbacks, velocity and drag interactions.
        var dx by remember { mutableFloatStateOf(0f) }
        var started by remember { mutableStateOf("-") }
        var stopped by remember { mutableStateOf("-") }
        val dragSource = remember { MutableInteractionSource() }
        val dragging by dragSource.collectIsDraggedAsState()
        Box(
            Modifier.size(200.dp, 40.dp).draggable(
                rememberDraggableState { dx += it }, Orientation.Horizontal, interactionSource = dragSource,
                onDragStarted = { started = "${it.x.roundToInt()}" }, onDragStopped = { stopped = if (it > 0f) "+" else if (it < 0f) "-v" else "0" },
            ),
        ) { Text("Drag ${dx.roundToInt()} $started $stopped $dragging") }
        // pointerInput restarts when its key changes.
        var key by remember { mutableIntStateOf(0) }
        var starts by remember { mutableIntStateOf(0) }
        Box(Modifier.size(200.dp, 40.dp).clickable { key++ }) { Text("Rekey $key") }
        Box(Modifier.size(200.dp, 40.dp).pointerInput(key) {
            starts++
            awaitPointerEventScope { while (true) awaitPointerEvent() }
        }) { Text("Starts $starts") }
        // Two-finger zoom.
        var zoom by remember { mutableFloatStateOf(1f) }
        Box(Modifier.size(200.dp, 40.dp).pointerInput(Unit) {
            detectTransformGestures { _, _, z, _ -> zoom *= z }
        }) { Text("Zoom ${(zoom * 10).roundToInt()}") }
        // Drag after a long press.
        var lp by remember { mutableStateOf("idle") }
        Box(Modifier.size(200.dp, 40.dp).pointerInput(Unit) {
            detectDragGesturesAfterLongPress(onDragStart = { lp = "start" }, onDragEnd = { lp = "$lp end" }) { c, _ -> c.consume(); if (!lp.contains("move")) lp = "$lp move" }
        }) { Text("AfterLong $lp") }
        // Scrollable: fling after release, and a press inside it shows after the tap timeout.
        var scrolled by remember { mutableFloatStateOf(0f) }
        val pressSource = remember { MutableInteractionSource() }
        val pressed by pressSource.collectIsPressedAsState()
        Box(Modifier.size(200.dp, 40.dp).scrollable(rememberScrollableState { scrolled += it; it }, Orientation.Horizontal)) {
            Box(Modifier.size(100.dp, 40.dp).clickable(interactionSource = pressSource) {}) {
                Text("Scroll ${scrolled.roundToInt()} $pressed")
            }
        }
        // Cancellation reaches the press; a disabled clickable takes no touches; no-wire modifiers.
        var cancels by remember { mutableIntStateOf(0) }
        val cancelSource = remember { MutableInteractionSource() }
        LaunchedEffect(cancelSource) { cancelSource.interactions.collect { if (it is PressInteraction.Cancel) cancels++ } }
        Box(Modifier.size(200.dp, 40.dp).clickable(interactionSource = cancelSource) {}) { Text("Cancels $cancels") }
        var disabled by remember { mutableIntStateOf(0) }
        Box(Modifier.size(200.dp, 40.dp).clickable(enabled = false) { disabled++ }) { Text("Disabled $disabled") }
        val hover = remember { MutableInteractionSource() }
        Box(Modifier.size(200.dp, 40.dp).hoverable(hover).selectableGroup()) { Text("Plain") }
        // Explicit unbounded ripple on another node, driven by a shared source.
        val shared = remember { MutableInteractionSource() }
        Box(Modifier.size(200.dp, 40.dp).clickable(shared, null) {}) {
            Box(Modifier.size(40.dp).indication(shared, ripple(bounded = false, radius = 20.dp, color = Color.Red))) { Text("Shared") }
        }
        // A throwing handler is reported; the app keeps running.
        var boom by remember { mutableIntStateOf(0) }
        Box(Modifier.size(200.dp, 40.dp).pointerInput(Unit) {
            awaitPointerEventScope { awaitPointerEvent() }
            boom++
            error("boom")
        }) { Text("Boom $boom") }
        // A user drag cancels a programmatic drag of lower priority.
        var prog by remember { mutableStateOf("idle") }
        val progState = rememberDraggableState { }
        LaunchedEffect(Unit) {
            try {
                progState.drag(MutatePriority.Default) { kotlinx.coroutines.awaitCancellation() }
            } catch (e: kotlinx.coroutines.CancellationException) {
                prog = "interrupted"
            }
        }
        Box(Modifier.size(200.dp, 40.dp).draggable(progState, Orientation.Horizontal)) { Text("Prog $prog") }
        // Work finishing on another thread updates the UI without further input.
        var background by remember { mutableStateOf("waiting") }
        var startBackground by remember { mutableStateOf(false) }
        Box(Modifier.size(200.dp, 40.dp).clickable { startBackground = true }) { Text("Background $background") }
        if (startBackground) {
            LaunchedEffect(Unit) {
                background = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { Thread.sleep(300); "done" }
            }
        }
        // Disabling a scrollable mid-drag ends the drag's mutation.
        var scrollOn by remember { mutableStateOf(true) }
        val toggled = rememberScrollableState { it }
        Box(Modifier.size(200.dp, 40.dp).scrollable(toggled, Orientation.Horizontal, enabled = scrollOn)) {
            Text("Toggled ${toggled.isScrollInProgress}")
        }
        Box(Modifier.size(200.dp, 40.dp).clickable { scrollOn = !scrollOn }) { Text("Switch scroll $scrollOn") }
    }
}
