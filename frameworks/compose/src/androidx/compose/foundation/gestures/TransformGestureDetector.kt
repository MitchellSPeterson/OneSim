package androidx.compose.foundation.gestures

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.PointerEvent
import androidx.compose.ui.input.pointer.PointerInputScope
import androidx.compose.ui.input.pointer.positionChanged
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2

/**
 * Pan, zoom and rotation from one or more fingers. [onGesture] gets the centroid, pan, zoom factor and rotation
 * (degrees) of each move once the touch slop is passed; with [panZoomLock] a rotation never starts during pan or zoom.
 */
suspend fun PointerInputScope.detectTransformGestures(
    panZoomLock: Boolean = false,
    onGesture: (centroid: Offset, pan: Offset, zoom: Float, rotation: Float) -> Unit,
) {
    awaitEachGesture {
        var rotation = 0f
        var zoom = 1f
        var pan = Offset.Zero
        var pastTouchSlop = false
        val touchSlop = viewConfiguration.touchSlop
        var lockedToPanZoom = false
        awaitFirstDown(requireUnconsumed = false)
        do {
            val event = awaitPointerEvent()
            val canceled = event.changes.any { it.isConsumed }
            if (!canceled) {
                val zoomChange = event.calculateZoom()
                val rotationChange = event.calculateRotation()
                val panChange = event.calculatePan()
                if (!pastTouchSlop) {
                    zoom *= zoomChange
                    rotation += rotationChange
                    pan += panChange
                    val centroidSize = event.calculateCentroidSize(useCurrent = false)
                    val zoomMotion = abs(1 - zoom) * centroidSize
                    val rotationMotion = abs(rotation * PI.toFloat() * centroidSize / 180f)
                    val panMotion = pan.getDistance()
                    if (zoomMotion > touchSlop || rotationMotion > touchSlop || panMotion > touchSlop) {
                        pastTouchSlop = true
                        lockedToPanZoom = panZoomLock && rotationMotion < touchSlop
                    }
                }
                if (pastTouchSlop) {
                    val centroid = event.calculateCentroid(useCurrent = false)
                    val effectiveRotation = if (lockedToPanZoom) 0f else rotationChange
                    if (effectiveRotation != 0f || zoomChange != 1f || panChange != Offset.Zero) {
                        onGesture(centroid, panChange, zoomChange, effectiveRotation)
                    }
                    event.changes.forEach { if (it.positionChanged()) it.consume() }
                }
            }
        } while (!canceled && event.changes.any { it.pressed })
    }
}

/** Mean of the pressed pointers' positions (current or previous), or [Offset.Unspecified] without any. */
fun PointerEvent.calculateCentroid(useCurrent: Boolean = true): Offset {
    var centroid = Offset.Zero
    var count = 0
    changes.forEach { c ->
        if (c.pressed && c.previousPressed) {
            centroid += if (useCurrent) c.position else c.previousPosition
            count++
        }
    }
    return if (count == 0) Offset.Unspecified else centroid / count.toFloat()
}

/** Mean distance of the pressed pointers from their centroid. */
fun PointerEvent.calculateCentroidSize(useCurrent: Boolean = true): Float {
    val centroid = calculateCentroid(useCurrent)
    if (centroid == Offset.Unspecified) return 0f
    var distanceToCentroid = 0f
    var count = 0
    changes.forEach { c ->
        if (c.pressed && c.previousPressed) {
            val position = if (useCurrent) c.position else c.previousPosition
            distanceToCentroid += (position - centroid).getDistance()
            count++
        }
    }
    return distanceToCentroid / count
}

/** Movement of the centroid. */
fun PointerEvent.calculatePan(): Offset {
    val current = calculateCentroid(useCurrent = true)
    if (current == Offset.Unspecified) return Offset.Zero
    return current - calculateCentroid(useCurrent = false)
}

/** Ratio of the pointers' spread now to before. */
fun PointerEvent.calculateZoom(): Float {
    val currentSize = calculateCentroidSize(useCurrent = true)
    val previousSize = calculateCentroidSize(useCurrent = false)
    if (currentSize == 0f || previousSize == 0f) return 1f
    return currentSize / previousSize
}

/** Rotation (degrees, clockwise) of the pointers around their centroid. */
fun PointerEvent.calculateRotation(): Float {
    val pointerCount = changes.count { it.previousPressed && it.pressed }
    if (pointerCount < 2) return 0f
    val currentCentroid = calculateCentroid(useCurrent = true)
    val previousCentroid = calculateCentroid(useCurrent = false)
    var rotation = 0f
    var rotationWeight = 0f
    changes.forEach { change ->
        if (change.pressed && change.previousPressed) {
            val currentPosition = change.position
            val previousPosition = change.previousPosition
            val previousOffset = previousPosition - previousCentroid
            val currentOffset = currentPosition - currentCentroid
            val previousAngle = previousOffset.angle()
            val currentAngle = currentOffset.angle()
            val angleDiff = currentAngle - previousAngle
            val weight = (currentOffset + previousOffset).getDistance() / 2f
            rotation += when {
                angleDiff > 180f -> angleDiff - 360f
                angleDiff < -180f -> angleDiff + 360f
                else -> angleDiff
            } * weight
            rotationWeight += weight
        }
    }
    return if (rotationWeight == 0f) 0f else rotation / rotationWeight
}

private fun Offset.angle(): Float = if (x == 0f && y == 0f) 0f else -atan2(x, y) * 180f / PI.toFloat()
