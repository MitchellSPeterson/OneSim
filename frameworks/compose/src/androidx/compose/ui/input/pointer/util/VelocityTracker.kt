package androidx.compose.ui.input.pointer.util

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.changedToDownIgnoreConsumed
import androidx.compose.ui.unit.Velocity
import kotlin.math.abs
import kotlin.math.sign

private const val AssumePointerMoveStoppedMilliseconds = 40
private const val HistorySize = 20
private const val HorizonMilliseconds = 100

/** Pointer velocity from recent positions (least-squares quadratic fit over the last 100 ms, as Compose's default). */
class VelocityTracker {
    private val xTracker = VelocityTracker1D(isDataDifferential = false, strategy = VelocityTracker1D.Strategy.Lsq2)
    private val yTracker = VelocityTracker1D(isDataDifferential = false, strategy = VelocityTracker1D.Strategy.Lsq2)
    internal var currentPointerPositionAccumulator = Offset.Zero
    internal var lastMoveEventTimeStamp = 0L

    /** Add a position (pixels) at [timeMillis]. */
    fun addPosition(timeMillis: Long, position: Offset) {
        xTracker.addDataPoint(timeMillis, position.x)
        yTracker.addDataPoint(timeMillis, position.y)
    }

    /** Velocity in pixels per second. */
    fun calculateVelocity(): Velocity = calculateVelocity(Velocity(Float.MAX_VALUE, Float.MAX_VALUE))

    /** Velocity in pixels per second, each axis clamped to [maximumVelocity]. */
    fun calculateVelocity(maximumVelocity: Velocity): Velocity {
        require(maximumVelocity.x > 0f && maximumVelocity.y > 0f) { "maximumVelocity should be a positive value. You specified=$maximumVelocity" }
        return Velocity(xTracker.calculateVelocity(maximumVelocity.x), yTracker.calculateVelocity(maximumVelocity.y))
    }

    fun resetTracking() {
        xTracker.resetTracking()
        yTracker.resetTracking()
        lastMoveEventTimeStamp = 0L
    }
}

/** Track [event] (resets at a down). */
fun VelocityTracker.addPointerInputChange(event: PointerInputChange) {
    if (event.changedToDownIgnoreConsumed()) {
        currentPointerPositionAccumulator = event.position
        resetTracking()
    }
    // Large gaps (stopped pointer) are handled by the 1D trackers; every change contributes its current position.
    currentPointerPositionAccumulator = event.position
    addPosition(event.uptimeMillis, currentPointerPositionAccumulator)
    lastMoveEventTimeStamp = event.uptimeMillis
}

/** Velocity of one coordinate. */
class VelocityTracker1D internal constructor(
    private val isDataDifferential: Boolean = false,
    private val strategy: Strategy = Strategy.Lsq2,
) {
    constructor(isDataDifferential: Boolean = false) : this(isDataDifferential, Strategy.Lsq2)

    internal enum class Strategy { Lsq2, Impulse }

    private class DataPointAtTime(var time: Long, var dataPoint: Float)

    private val minSampleSize = if (strategy == Strategy.Lsq2) 3 else 2
    private val samples = arrayOfNulls<DataPointAtTime>(HistorySize)
    private var index = 0
    private val reusableDataPointsArray = FloatArray(HistorySize)
    private val reusableTimeArray = FloatArray(HistorySize)

    fun addDataPoint(timeMillis: Long, dataPoint: Float) {
        index = (index + 1) % HistorySize
        val s = samples[index]
        if (s == null) samples[index] = DataPointAtTime(timeMillis, dataPoint) else {
            s.time = timeMillis
            s.dataPoint = dataPoint
        }
    }

    fun resetTracking() {
        samples.fill(null)
        index = 0
    }

    /** Units per second. */
    fun calculateVelocity(): Float {
        val dataPoints = reusableDataPointsArray
        val time = reusableTimeArray
        var sampleCount = 0
        var i = index
        val newestSample = samples[i] ?: return 0f
        var previousSample: DataPointAtTime = newestSample
        do {
            val sample = samples[i] ?: break
            val age = (newestSample.time - sample.time).toFloat()
            val delta = abs(sample.time - previousSample.time).toFloat()
            previousSample = if (strategy == Strategy.Lsq2 || isDataDifferential) sample else newestSample
            if (age > HorizonMilliseconds || delta > AssumePointerMoveStoppedMilliseconds) break
            dataPoints[sampleCount] = sample.dataPoint
            time[sampleCount] = -age
            i = (if (i == 0) HistorySize else i) - 1
            sampleCount++
        } while (sampleCount < HistorySize)
        if (sampleCount >= minSampleSize) {
            return when (strategy) {
                Strategy.Impulse -> calculateImpulseVelocity(dataPoints, time, sampleCount, isDataDifferential)
                Strategy.Lsq2 -> calculateLeastSquaresVelocity(dataPoints, time, sampleCount)
            } * 1000f
        }
        return 0f
    }

    fun calculateVelocity(maximumVelocity: Float): Float {
        require(maximumVelocity > 0f) { "maximumVelocity should be a positive value. You specified=$maximumVelocity" }
        val velocity = calculateVelocity()
        return if (velocity == 0.0f || velocity.isNaN()) 0.0f
        else if (velocity > 0) velocity.coerceAtMost(maximumVelocity)
        else velocity.coerceAtLeast(-maximumVelocity)
    }

    private fun calculateLeastSquaresVelocity(dataPoints: FloatArray, time: FloatArray, sampleCount: Int): Float = try {
        polyFitLeastSquares(time, dataPoints, sampleCount, 2)[1]
    } catch (_: IllegalArgumentException) {
        0f
    }
}

private fun calculateImpulseVelocity(dataPoints: FloatArray, time: FloatArray, sampleCount: Int, isDataDifferential: Boolean): Float {
    var work = 0f
    val start = sampleCount - 1
    var nextTime = time[start]
    for (i in start downTo 1) {
        val currentTime = nextTime
        nextTime = time[i - 1]
        if (currentTime == nextTime) continue
        val dataPointsDelta = if (isDataDifferential) -dataPoints[i - 1] else dataPoints[i] - dataPoints[i - 1]
        val vCurr = dataPointsDelta / (currentTime - nextTime)
        val vPrev = kineticEnergyToVelocity(work)
        work += (vCurr - vPrev) * abs(vCurr)
        if (i == start) work *= 0.5f
    }
    return kineticEnergyToVelocity(work)
}

private fun kineticEnergyToVelocity(kineticEnergy: Float): Float = sign(kineticEnergy) * kotlin.math.sqrt(2 * abs(kineticEnergy))

/**
 * Least-squares polynomial fit of [y] over [x] (first [sampleCount] samples) with the given [degree]: the coefficients,
 * lowest order first. Solved with Householder QR, as Android's VelocityTracker.
 */
internal fun polyFitLeastSquares(x: FloatArray, y: FloatArray, sampleCount: Int, degree: Int): FloatArray {
    require(degree >= 1) { "The degree must be at positive integer" }
    require(sampleCount != 0) { "At least one point must be provided" }
    val truncatedDegree = if (degree >= sampleCount) sampleCount - 1 else degree
    val n = truncatedDegree + 1
    // Expand the X vector to a matrix A, pre-multiplied by the weights.
    val a = Array(n) { FloatArray(sampleCount) }
    for (h in 0 until sampleCount) {
        a[0][h] = 1f
        for (i in 1 until n) a[i][h] = a[i - 1][h] * x[h]
    }
    // Gram-Schmidt orthonormalization: A = QR.
    val q = Array(n) { FloatArray(sampleCount) }
    val r = Array(n) { FloatArray(n) }
    for (j in 0 until n) {
        for (h in 0 until sampleCount) q[j][h] = a[j][h]
        for (i in 0 until j) {
            val dot = dot(q[j], q[i], sampleCount)
            for (h in 0 until sampleCount) q[j][h] -= dot * q[i][h]
        }
        val norm = kotlin.math.sqrt(dot(q[j], q[j], sampleCount))
        require(norm >= 0.000001f) { "Vectors are linearly dependent or zero so no solution." }
        val inverseNorm = 1.0f / norm
        for (h in 0 until sampleCount) q[j][h] *= inverseNorm
        for (i in 0 until n) r[j][i] = if (i < j) 0.0f else dot(q[j], a[i], sampleCount)
    }
    // Solve R B = Qt Y for B (back substitution).
    val coefficients = FloatArray(degree + 1)
    for (i in n - 1 downTo 0) {
        coefficients[i] = dot(q[i], y, sampleCount)
        for (j in n - 1 downTo i + 1) coefficients[i] -= r[i][j] * coefficients[j]
        coefficients[i] /= r[i][i]
    }
    return coefficients
}

private fun dot(a: FloatArray, b: FloatArray, n: Int): Float {
    var s = 0f
    for (i in 0 until n) s += a[i] * b[i]
    return s
}
