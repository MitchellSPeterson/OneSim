package androidx.compose.ui.graphics

import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.jsonNum

enum class PathFillType { NonZero, EvenOdd }

class Path {
    private data class Command(val op: String, val points: List<Float>)
    private val commands = mutableListOf<Command>()
    var fillType: PathFillType = PathFillType.NonZero

    val isEmpty: Boolean get() = commands.isEmpty()

    private fun add(op: String, vararg points: Float) {
        commands.add(Command(op, points.toList()))
    }

    fun moveTo(x: Float, y: Float) = add("m", x, y)
    fun lineTo(x: Float, y: Float) = add("l", x, y)
    fun quadraticTo(x1: Float, y1: Float, x2: Float, y2: Float) = add("q", x1, y1, x2, y2)
    fun quadraticBezierTo(x1: Float, y1: Float, x2: Float, y2: Float) = quadraticTo(x1, y1, x2, y2)
    fun cubicTo(x1: Float, y1: Float, x2: Float, y2: Float, x3: Float, y3: Float) = add("c", x1, y1, x2, y2, x3, y3)
    fun close() = add("z")

    fun addRect(rect: Rect) {
        moveTo(rect.left, rect.top)
        lineTo(rect.right, rect.top)
        lineTo(rect.right, rect.bottom)
        lineTo(rect.left, rect.bottom)
        close()
    }

    fun reset() { commands.clear() }
    fun rewind() = reset()

    fun getBounds(): Rect {
        val points = commands.flatMap { it.points }.chunked(2)
        if (points.isEmpty()) return Rect.Zero
        return Rect(points.minOf { it[0] }, points.minOf { it[1] }, points.maxOf { it[0] }, points.maxOf { it[1] })
    }

    internal fun wire(density: Float): String {
        val path = commands.joinToString(prefix = "[", postfix = "]") { command ->
            if (command.points.isEmpty()) "\"z\"" else {
                val points = command.points.joinToString(prefix = "[", postfix = "]") { jsonNum(it / density) }
                """{"${command.op}":$points}"""
            }
        }
        return """{"path":$path,"eoFill":${fillType == PathFillType.EvenOdd}}"""
    }
}
