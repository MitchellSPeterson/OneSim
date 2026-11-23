package androidx.compose.animation

import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.sampleSpec
import androidx.compose.animation.core.spring
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.Stable
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize

/** Fade part of a transition: to or from [alpha]. */
@Immutable
class Fade(val alpha: Float, val animationSpec: FiniteAnimationSpec<Float>)

/** Slide part: to or from the offset [slideOffset] gives for the content's full size (pixels). */
@Immutable
class Slide(val slideOffset: (fullSize: IntSize) -> IntOffset, val animationSpec: FiniteAnimationSpec<IntOffset>)

/** Scale part: to or from [scale] around [transformOrigin]. */
@Immutable
class Scale(val scale: Float, val transformOrigin: TransformOrigin, val animationSpec: FiniteAnimationSpec<Float>)

/** Size-change part (`expandIn` / `shrinkOut`): the clip grows from or shrinks to [size]. */
@Immutable
class ChangeSize(val alignment: Alignment, val size: (fullSize: IntSize) -> IntSize, val animationSpec: FiniteAnimationSpec<IntSize>, val clip: Boolean = true)

@Immutable
internal data class TransitionData(
    val fade: Fade? = null,
    val slide: Slide? = null,
    val changeSize: ChangeSize? = null,
    val scale: Scale? = null,
    val hold: Boolean = false,
)

/** How content appears: any of fade, slide, scale and expand, combined with `+` (the left side wins per part). */
@Immutable
sealed class EnterTransition {
    internal abstract val data: TransitionData

    @Stable
    operator fun plus(enter: EnterTransition): EnterTransition = EnterTransitionImpl(
        TransitionData(data.fade ?: enter.data.fade, data.slide ?: enter.data.slide, data.changeSize ?: enter.data.changeSize, data.scale ?: enter.data.scale),
    )

    override fun equals(other: Any?) = other is EnterTransition && other.data == data
    override fun hashCode() = data.hashCode()
    override fun toString() = if (this == None) "EnterTransition.None" else "EnterTransition: $data"

    companion object {
        val None: EnterTransition = EnterTransitionImpl(TransitionData())
    }
}

/** How content disappears; see [EnterTransition]. */
@Immutable
sealed class ExitTransition {
    internal abstract val data: TransitionData

    @Stable
    operator fun plus(exit: ExitTransition): ExitTransition = ExitTransitionImpl(
        TransitionData(data.fade ?: exit.data.fade, data.slide ?: exit.data.slide, data.changeSize ?: exit.data.changeSize, data.scale ?: exit.data.scale, data.hold || exit.data.hold),
    )

    override fun equals(other: Any?) = other is ExitTransition && other.data == data
    override fun hashCode() = data.hashCode()
    override fun toString() = when (this) {
        None -> "ExitTransition.None"
        KeepUntilTransitionsFinished -> "ExitTransition.KeepUntilTransitionsFinished"
        else -> "ExitTransition: $data"
    }

    companion object {
        val None: ExitTransition = ExitTransitionImpl(TransitionData())
        /** Keeps the outgoing content, unchanged, until the incoming content's transition finishes. */
        val KeepUntilTransitionsFinished: ExitTransition = ExitTransitionImpl(TransitionData(hold = true))
    }
}

private class EnterTransitionImpl(override val data: TransitionData) : EnterTransition()
private class ExitTransitionImpl(override val data: TransitionData) : ExitTransition()

private fun <T> defaultSpring(threshold: T? = null) = spring(stiffness = Spring.StiffnessMediumLow, visibilityThreshold = threshold)

@Stable
fun fadeIn(animationSpec: FiniteAnimationSpec<Float> = defaultSpring(), initialAlpha: Float = 0f): EnterTransition =
    EnterTransitionImpl(TransitionData(fade = Fade(initialAlpha, animationSpec)))

@Stable
fun fadeOut(animationSpec: FiniteAnimationSpec<Float> = defaultSpring(), targetAlpha: Float = 0f): ExitTransition =
    ExitTransitionImpl(TransitionData(fade = Fade(targetAlpha, animationSpec)))

@Stable
fun slideIn(animationSpec: FiniteAnimationSpec<IntOffset> = defaultSpring(IntOffset(1, 1)), initialOffset: (fullSize: IntSize) -> IntOffset): EnterTransition =
    EnterTransitionImpl(TransitionData(slide = Slide(initialOffset, animationSpec)))

@Stable
fun slideOut(animationSpec: FiniteAnimationSpec<IntOffset> = defaultSpring(IntOffset(1, 1)), targetOffset: (fullSize: IntSize) -> IntOffset): ExitTransition =
    ExitTransitionImpl(TransitionData(slide = Slide(targetOffset, animationSpec)))

@Stable
fun slideInHorizontally(animationSpec: FiniteAnimationSpec<IntOffset> = defaultSpring(IntOffset(1, 1)), initialOffsetX: (fullWidth: Int) -> Int = { -it / 2 }): EnterTransition =
    slideIn(animationSpec) { IntOffset(initialOffsetX(it.width), 0) }

@Stable
fun slideInVertically(animationSpec: FiniteAnimationSpec<IntOffset> = defaultSpring(IntOffset(1, 1)), initialOffsetY: (fullHeight: Int) -> Int = { -it / 2 }): EnterTransition =
    slideIn(animationSpec) { IntOffset(0, initialOffsetY(it.height)) }

@Stable
fun slideOutHorizontally(animationSpec: FiniteAnimationSpec<IntOffset> = defaultSpring(IntOffset(1, 1)), targetOffsetX: (fullWidth: Int) -> Int = { -it / 2 }): ExitTransition =
    slideOut(animationSpec) { IntOffset(targetOffsetX(it.width), 0) }

@Stable
fun slideOutVertically(animationSpec: FiniteAnimationSpec<IntOffset> = defaultSpring(IntOffset(1, 1)), targetOffsetY: (fullHeight: Int) -> Int = { -it / 2 }): ExitTransition =
    slideOut(animationSpec) { IntOffset(0, targetOffsetY(it.height)) }

@Stable
fun scaleIn(animationSpec: FiniteAnimationSpec<Float> = defaultSpring(), initialScale: Float = 0f, transformOrigin: TransformOrigin = TransformOrigin.Center): EnterTransition =
    EnterTransitionImpl(TransitionData(scale = Scale(initialScale, transformOrigin, animationSpec)))

@Stable
fun scaleOut(animationSpec: FiniteAnimationSpec<Float> = defaultSpring(), targetScale: Float = 0f, transformOrigin: TransformOrigin = TransformOrigin.Center): ExitTransition =
    ExitTransitionImpl(TransitionData(scale = Scale(targetScale, transformOrigin, animationSpec)))

@Stable
fun expandIn(animationSpec: FiniteAnimationSpec<IntSize> = defaultSpring(IntSize(1, 1)), expandFrom: Alignment = Alignment.BottomEnd, clip: Boolean = true, initialSize: (fullSize: IntSize) -> IntSize = { IntSize(0, 0) }): EnterTransition =
    EnterTransitionImpl(TransitionData(changeSize = ChangeSize(expandFrom, initialSize, animationSpec, clip)))

@Stable
fun shrinkOut(animationSpec: FiniteAnimationSpec<IntSize> = defaultSpring(IntSize(1, 1)), shrinkTowards: Alignment = Alignment.BottomEnd, clip: Boolean = true, targetSize: (fullSize: IntSize) -> IntSize = { IntSize(0, 0) }): ExitTransition =
    ExitTransitionImpl(TransitionData(changeSize = ChangeSize(shrinkTowards, targetSize, animationSpec, clip)))

@Stable
fun expandHorizontally(animationSpec: FiniteAnimationSpec<IntSize> = defaultSpring(IntSize(1, 1)), expandFrom: Alignment.Horizontal = Alignment.End, clip: Boolean = true, initialWidth: (fullWidth: Int) -> Int = { 0 }): EnterTransition =
    expandIn(animationSpec, if (expandFrom == Alignment.Start) Alignment.CenterStart else Alignment.CenterEnd, clip) { IntSize(initialWidth(it.width), it.height) }

@Stable
fun expandVertically(animationSpec: FiniteAnimationSpec<IntSize> = defaultSpring(IntSize(1, 1)), expandFrom: Alignment.Vertical = Alignment.Bottom, clip: Boolean = true, initialHeight: (fullHeight: Int) -> Int = { 0 }): EnterTransition =
    expandIn(animationSpec, if (expandFrom == Alignment.Top) Alignment.TopCenter else Alignment.BottomCenter, clip) { IntSize(it.width, initialHeight(it.height)) }

@Stable
fun shrinkHorizontally(animationSpec: FiniteAnimationSpec<IntSize> = defaultSpring(IntSize(1, 1)), shrinkTowards: Alignment.Horizontal = Alignment.End, clip: Boolean = true, targetWidth: (fullWidth: Int) -> Int = { 0 }): ExitTransition =
    shrinkOut(animationSpec, if (shrinkTowards == Alignment.Start) Alignment.CenterStart else Alignment.CenterEnd, clip) { IntSize(targetWidth(it.width), it.height) }

@Stable
fun shrinkVertically(animationSpec: FiniteAnimationSpec<IntSize> = defaultSpring(IntSize(1, 1)), shrinkTowards: Alignment.Vertical = Alignment.Bottom, clip: Boolean = true, targetHeight: (fullHeight: Int) -> Int = { 0 }): ExitTransition =
    shrinkOut(animationSpec, if (shrinkTowards == Alignment.Top) Alignment.TopCenter else Alignment.BottomCenter, clip) { IntSize(it.width, targetHeight(it.height)) }

/**
 * The look of entering or exiting content [elapsedMs] into its transition: alpha, offset in pixels and scale, and
 * whether every part finished. `entering` animates from the transition's values to identity, exiting the reverse.
 */
internal class TransitionFrame(val alpha: Float, val offset: IntOffset, val scale: Float, val done: Boolean)

internal fun transitionFrame(data: TransitionData, entering: Boolean, fullSize: IntSize, elapsedMs: Float): TransitionFrame {
    val nanos = (elapsedMs * 1_000_000f).toLong()
    var done = true
    fun anim(spec: FiniteAnimationSpec<*>, a: FloatArray, b: FloatArray, threshold: Float): FloatArray {
        val (from, to) = if (entering) a to b else b to a
        val (v, _, end) = sampleSpec(spec, from, to, FloatArray(from.size), nanos) { threshold }
        if (!end) done = false
        return v
    }
    val alpha = data.fade?.let { anim(it.animationSpec, floatArrayOf(it.alpha), floatArrayOf(1f), 0.01f)[0] } ?: 1f
    val offset = data.slide?.let {
        val o = it.slideOffset(fullSize)
        val v = anim(it.animationSpec, floatArrayOf(o.x.toFloat(), o.y.toFloat()), floatArrayOf(0f, 0f), 1f)
        IntOffset(kotlin.math.round(v[0]).toInt(), kotlin.math.round(v[1]).toInt())
    } ?: IntOffset.Zero
    val scale = data.scale?.let { anim(it.animationSpec, floatArrayOf(it.scale), floatArrayOf(1f), 0.01f)[0] } ?: 1f
    data.changeSize?.let {
        val s = it.size(fullSize)
        anim(it.animationSpec, floatArrayOf(s.width.toFloat(), s.height.toFloat()), floatArrayOf(fullSize.width.toFloat(), fullSize.height.toFloat()), 1f)
    }
    return TransitionFrame(alpha.coerceIn(0f, 1f), offset, scale, done)
}

/** Scope of the transitions of an animated content change from [initialState] to [targetState]. */
interface AnimatedContentTransitionScope<S> {
    val initialState: S
    val targetState: S

    infix fun S.isTransitioningTo(targetState: S): Boolean = this == initialState && targetState == this@AnimatedContentTransitionScope.targetState

    /** Slide in from the container's edge in [towards] (Left = from the right edge, moving left). */
    fun slideIntoContainer(
        towards: SlideDirection,
        animationSpec: FiniteAnimationSpec<IntOffset> = spring(visibilityThreshold = IntOffset(1, 1)),
        initialOffset: (offsetForFullSlide: Int) -> Int = { it },
    ): EnterTransition = when (towards) {
        SlideDirection.Left, SlideDirection.Start -> slideInHorizontally(animationSpec) { initialOffset(it) }
        SlideDirection.Right, SlideDirection.End -> slideInHorizontally(animationSpec) { -initialOffset(it) }
        SlideDirection.Up -> slideInVertically(animationSpec) { initialOffset(it) }
        else -> slideInVertically(animationSpec) { -initialOffset(it) }
    }

    fun slideOutOfContainer(
        towards: SlideDirection,
        animationSpec: FiniteAnimationSpec<IntOffset> = spring(visibilityThreshold = IntOffset(1, 1)),
        targetOffset: (offsetForFullSlide: Int) -> Int = { it },
    ): ExitTransition = when (towards) {
        SlideDirection.Left, SlideDirection.Start -> slideOutHorizontally(animationSpec) { -targetOffset(it) }
        SlideDirection.Right, SlideDirection.End -> slideOutHorizontally(animationSpec) { targetOffset(it) }
        SlideDirection.Up -> slideOutVertically(animationSpec) { -targetOffset(it) }
        else -> slideOutVertically(animationSpec) { targetOffset(it) }
    }

    infix fun EnterTransition.togetherWith(exit: ExitTransition): ContentTransform = ContentTransform(this, exit)

    @kotlin.jvm.JvmInline
    value class SlideDirection internal constructor(private val value: Int) {
        companion object {
            val Left = SlideDirection(0)
            val Right = SlideDirection(1)
            val Up = SlideDirection(2)
            val Down = SlideDirection(3)
            val Start = SlideDirection(4)
            val End = SlideDirection(5)
        }
    }
}

/** Enter and exit transitions of one content change, drawn with [targetContentZIndex]. */
class ContentTransform(val targetContentEnter: EnterTransition, val initialContentExit: ExitTransition, targetContentZIndex: Float = 0f, sizeTransform: SizeTransform? = null) {
    var targetContentZIndex: Float = targetContentZIndex
        internal set
    var sizeTransform: SizeTransform? = sizeTransform
        internal set
}

/** How the container's size animates during a content change (OneSim: the container takes the new size at once). */
interface SizeTransform {
    val clip: Boolean
}

fun SizeTransform(clip: Boolean = true, sizeAnimationSpec: (initialSize: IntSize, targetSize: IntSize) -> FiniteAnimationSpec<IntSize> = { _, _ -> spring(visibilityThreshold = IntSize(1, 1)) }): SizeTransform =
    object : SizeTransform { override val clip = clip }

infix fun EnterTransition.togetherWith(exit: ExitTransition): ContentTransform = ContentTransform(this, exit)

infix fun ContentTransform.using(sizeTransform: SizeTransform?): ContentTransform = this.apply { this.sizeTransform = sizeTransform }
