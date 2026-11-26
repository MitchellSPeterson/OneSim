package androidx.compose.material3

import androidx.compose.foundation.IndicationNodeState
import androidx.compose.foundation.IndicationRenderer
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.interaction.FocusInteraction
import androidx.compose.foundation.interaction.HoverInteraction
import androidx.compose.foundation.interaction.Interaction
import androidx.compose.foundation.interaction.InteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.material.ripple.RippleAlpha
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorProducer
import androidx.compose.ui.jsonNum
import androidx.compose.ui.unit.Dp
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import onesim.compose.Guest
import onesim.compose.WireCx

/**
 * Material 3 ripple: a circle growing from the press point (bounded: clipped to the component, ending 10 dp past its
 * corners; unbounded: [radius] or half the diagonal, centred) plus state layers for drag, focus and hover.
 * [color] defaults to [LocalRippleConfiguration]'s, then [LocalContentColor].
 */
@Stable
fun ripple(bounded: Boolean = true, radius: Dp = Dp.Unspecified, color: Color = Color.Unspecified): androidx.compose.foundation.Indication =
    RippleIndication(bounded, radius, color, null)

/** [ripple] whose color is computed at draw time. */
@Stable
fun ripple(color: ColorProducer, bounded: Boolean = true, radius: Dp = Dp.Unspecified): androidx.compose.foundation.Indication =
    RippleIndication(bounded, radius, Color.Unspecified, color)

object RippleDefaults {
    /** Material 3 state layer opacities: pressed 10%, focused 10%, dragged 16%, hovered 8%. */
    val RippleAlpha: RippleAlpha = RippleAlpha(draggedAlpha = 0.16f, focusedAlpha = 0.1f, hoveredAlpha = 0.08f, pressedAlpha = 0.1f)
}

/** Ripple color and alpha for a subtree; `null` in [LocalRippleConfiguration] disables ripples. */
@Immutable
class RippleConfiguration(val color: Color = Color.Unspecified, val rippleAlpha: RippleAlpha? = null) {
    override fun equals(other: Any?) = other is RippleConfiguration && color == other.color && rippleAlpha == other.rippleAlpha
    override fun hashCode() = color.hashCode() * 31 + rippleAlpha.hashCode()
}

val LocalRippleConfiguration = compositionLocalOf<RippleConfiguration?> { RippleConfiguration() }

private class RippleIndication(
    val bounded: Boolean,
    val radius: Dp,
    val color: Color,
    val colorProducer: ColorProducer?,
) : IndicationRenderer {
    override fun createState(): IndicationNodeState = RippleState(this)

    override fun equals(other: Any?) = other is RippleIndication && bounded == other.bounded && radius == other.radius &&
        color == other.color && colorProducer == other.colorProducer
    override fun hashCode() = ((bounded.hashCode() * 31 + radius.hashCode()) * 31 + color.hashCode()) * 31 + colorProducer.hashCode()
}

private const val FadeInMs = 75L
private const val RadiusMs = 225L
private const val FadeOutMs = 150L

/** `FastOutSlowInEasing` = cubic Bézier (0.4, 0, 0.2, 1). */
private fun fastOutSlowIn(t: Float): Float {
    if (t <= 0f || t >= 1f) return t.coerceIn(0f, 1f)
    fun bezier(a: Float, b: Float, m: Float) = 3 * a * (1 - m) * (1 - m) * m + 3 * b * (1 - m) * m * m + m * m * m
    var lo = 0f
    var hi = 1f
    repeat(30) {
        val mid = (lo + hi) / 2
        if (bezier(0.4f, 0.2f, mid) < t) lo = mid else hi = mid
    }
    return bezier(0f, 1f, (lo + hi) / 2)
}

/** One press's ripple, timed by frame nanos. */
private class RippleAnimation(val press: PressInteraction.Press, val origin: Offset) {
    var start = -1L
    var released = false
    var releaseAt = -1L

    private fun ms(now: Long, from: Long) = (now - from) / 1_000_000f

    /** (radius fraction, centre fraction, alpha fraction) at frame [now]. */
    fun at(now: Long): Triple<Float, Float, Float> {
        if (start < 0) return Triple(0f, 0f, 0f)
        val el = ms(now, start)
        val radius = fastOutSlowIn((el / RadiusMs).coerceIn(0f, 1f))
        val centre = (el / RadiusMs).coerceIn(0f, 1f)
        var alpha = (el / FadeInMs).coerceIn(0f, 1f)
        val fade = fadeStart()
        if (fade >= 0 && now >= fade) alpha *= 1f - (ms(now, fade) / FadeOutMs).coerceIn(0f, 1f)
        return Triple(radius, centre, alpha)
    }

    /** The fade out starts at the release, but not before the enter animation (225 ms) finishes. */
    private fun fadeStart(): Long {
        if (!released || start < 0) return -1L
        return maxOf(if (releaseAt >= 0) releaseAt else start, start + RadiusMs * 1_000_000L)
    }

    /** Fully grown and still held: nothing changes until the release. */
    fun settled(now: Long) = !released && start >= 0 && now - start >= RadiusMs * 1_000_000L

    fun finished(now: Long): Boolean {
        val f = fadeStart()
        return f >= 0 && now >= f + FadeOutMs * 1_000_000L
    }
}

private class RippleState(private val indication: RippleIndication) : IndicationNodeState() {
    private var source: InteractionSource? = null
    private var collector: Job? = null
    private var ticker: Job? = null
    private val ripples = ArrayList<RippleAnimation>()
    /** The latest frame time; written every frame while animating so the tree is re-sent. */
    private val frame = mutableLongStateOf(0L)
    private val layers = ArrayList<Interaction>()
    /** State layer: alpha it animates from, target, and when that animation started. */
    private var layerFrom = 0f
    private var layerTo = 0f
    private var layerStart = -1L
    private var layerMs = 15L

    override fun bind(source: InteractionSource) {
        if (source === this.source) return
        stop()
        this.source = source
        collector = Guest.scope.launch { source.interactions.collect { on(it) } }
    }

    private fun on(i: Interaction) {
        when (i) {
            is PressInteraction.Press -> ripples.add(RippleAnimation(i, i.pressPosition))
            is PressInteraction.Release -> release(i.press)
            is PressInteraction.Cancel -> release(i.press)
            is HoverInteraction.Enter, is FocusInteraction.Focus, is DragInteraction.Start -> layer(i, add = true)
            is HoverInteraction.Exit -> layer(i.enter, add = false)
            is FocusInteraction.Unfocus -> layer(i.focus, add = false)
            is DragInteraction.Stop -> layer(i.start, add = false)
            is DragInteraction.Cancel -> layer(i.start, add = false)
        }
        tick()
    }

    private fun release(press: PressInteraction.Press) {
        ripples.firstOrNull { it.press === press && !it.released }?.let {
            it.released = true
            it.releaseAt = if (it.start >= 0) frame.longValue else -1L
        }
    }

    private fun layer(i: Interaction, add: Boolean) {
        val now = frame.longValue
        layerFrom = layerAlpha(now)
        if (add) layers.add(i) else layers.remove(i)
        val top = layers.lastOrNull()
        layerTo = top?.let { alphaFor(it) } ?: 0f
        // Incoming: 15 ms (hover), 45 ms (focus, drag); outgoing: 150 ms after a drag, else 15 ms.
        layerMs = if (add) (if (i is HoverInteraction.Enter) 15L else 45L) else (if (i is DragInteraction.Start) 150L else 15L)
        layerStart = -1L
    }

    private fun alphaFor(i: Interaction): Float {
        val a = rippleAlpha()
        return when (i) {
            is HoverInteraction.Enter -> a.hoveredAlpha
            is FocusInteraction.Focus -> a.focusedAlpha
            is DragInteraction.Start -> a.draggedAlpha
            else -> 0f
        }
    }

    private var config: RippleConfiguration? = RippleConfiguration()

    private fun rippleAlpha(): RippleAlpha = config?.rippleAlpha ?: RippleDefaults.RippleAlpha

    private fun layerAlpha(now: Long): Float {
        if (layerStart < 0) return if (layerFrom == layerTo) layerTo else layerFrom
        val p = ((now - layerStart) / 1_000_000f / layerMs).coerceIn(0f, 1f)
        return layerFrom + (layerTo - layerFrom) * p
    }

    private fun animating(now: Long) =
        ripples.any { !it.settled(now) } || (layerFrom != layerTo && (layerStart < 0 || now - layerStart < layerMs * 1_000_000L))

    private fun tick() {
        if (ticker?.isActive == true) return
        ticker = Guest.scope.launch {
            while (true) {
                val now = withFrameNanos { it }
                ripples.forEach { if (it.start < 0) it.start = now }
                ripples.forEach { if (it.released && it.releaseAt < 0 && it.start >= 0) it.releaseAt = now }
                if (layerStart < 0 && layerFrom != layerTo) layerStart = now
                ripples.removeAll { it.finished(now) }
                frame.longValue = now
                if (!animating(now)) {
                    layerFrom = layerTo
                    break
                }
            }
        }
    }

    override fun wire(cx: WireCx) {
        config = cx.local(LocalRippleConfiguration)
        if (config == null) return
        val now = frame.longValue
        val layer = layerAlpha(now)
        if (ripples.isEmpty() && layer <= 0f) return
        val base = indication.colorProducer?.invoke()?.takeIf { it.isSpecified }
            ?: indication.color.takeIf { it.isSpecified }
            ?: config?.color?.takeIf { it.isSpecified }
            ?: cx.local(LocalContentColor)
        val pressed = rippleAlpha().pressedAlpha
        val d = Guest.density
        val circles = ripples.joinToString(",") { r ->
            val (radius, centre, alpha) = r.at(now)
            val origin = if (indication.bounded) "[${jsonNum(r.origin.x / d)},${jsonNum(r.origin.y / d)}]" else "null"
            """{"origin":$origin,"radius":${jsonNum(radius)},"center":${jsonNum(centre)},"alpha":${jsonNum(alpha * pressed)}}"""
        }
        val radius = if (indication.radius.isSpecified) ""","radius":${jsonNum(indication.radius.value)}""" else ""
        val color = "[${jsonNum(base.red)},${jsonNum(base.green)},${jsonNum(base.blue)},1]"
        cx.add("""{"ripple":{"color":$color,"bounded":${indication.bounded}$radius,"layer":${jsonNum(layer)},"ripples":[$circles]}}""")
    }

    private fun stop() {
        collector?.cancel()
        ticker?.cancel()
        collector = null
        ticker = null
        ripples.clear()
        layers.clear()
        layerFrom = 0f
        layerTo = 0f
    }

    override fun detach() {
        stop()
        source = null
    }
}
