package androidx.compose.foundation.layout

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.Stable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.jsonNum
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.wire
import onesim.compose.Guest

private fun n(d: Dp) = jsonNum(d.value)

@Stable
fun Modifier.padding(start: Dp = 0.dp, top: Dp = 0.dp, end: Dp = 0.dp, bottom: Dp = 0.dp): Modifier =
    wire("""{"padding":[${n(start)},${n(top)},${n(end)},${n(bottom)}]}""")

@Stable
fun Modifier.padding(horizontal: Dp = 0.dp, vertical: Dp = 0.dp): Modifier = padding(horizontal, vertical, horizontal, vertical)

@Stable
fun Modifier.padding(all: Dp): Modifier = padding(all, all, all, all)

@Stable
fun Modifier.padding(paddingValues: PaddingValues): Modifier = padding(
    paddingValues.calculateStartPadding(), paddingValues.calculateTopPadding(),
    paddingValues.calculateEndPadding(), paddingValues.calculateBottomPadding(),
)

@Stable
fun Modifier.absolutePadding(left: Dp = 0.dp, top: Dp = 0.dp, right: Dp = 0.dp, bottom: Dp = 0.dp): Modifier = padding(left, top, right, bottom)

@Immutable
class PaddingValues(private val start: Dp = 0.dp, private val top: Dp = 0.dp, private val end: Dp = 0.dp, private val bottom: Dp = 0.dp) {
    constructor(all: Dp) : this(all, all, all, all)
    constructor(horizontal: Dp = 0.dp, vertical: Dp = 0.dp) : this(horizontal, vertical, horizontal, vertical)

    fun calculateStartPadding(layoutDirection: Any? = null) = start
    fun calculateEndPadding(layoutDirection: Any? = null) = end
    fun calculateTopPadding() = top
    fun calculateBottomPadding() = bottom
    override fun equals(other: Any?) = other is PaddingValues && start == other.start && top == other.top && end == other.end && bottom == other.bottom
    override fun hashCode() = ((start.hashCode() * 31 + top.hashCode()) * 31 + end.hashCode()) * 31 + bottom.hashCode()
    override fun toString() = "PaddingValues(start=$start, top=$top, end=$end, bottom=$bottom)"
}

@Stable fun Modifier.width(width: Dp): Modifier = if (width.isUnspecified) this else wire("""{"width":${n(width)}}""")
@Stable fun Modifier.height(height: Dp): Modifier = if (height.isUnspecified) this else wire("""{"height":${n(height)}}""")
@Stable fun Modifier.size(size: Dp): Modifier = width(size).height(size)
@Stable fun Modifier.size(width: Dp, height: Dp): Modifier = width(width).height(height)
@Stable fun Modifier.size(size: DpSize): Modifier = size(size.width, size.height)
@Stable fun Modifier.widthIn(min: Dp = Dp.Unspecified, max: Dp = Dp.Unspecified): Modifier = wire("""{"widthIn":[${n(min)},${n(max)}]}""")
@Stable fun Modifier.heightIn(min: Dp = Dp.Unspecified, max: Dp = Dp.Unspecified): Modifier = wire("""{"heightIn":[${n(min)},${n(max)}]}""")
@Stable fun Modifier.sizeIn(minWidth: Dp = Dp.Unspecified, minHeight: Dp = Dp.Unspecified, maxWidth: Dp = Dp.Unspecified, maxHeight: Dp = Dp.Unspecified): Modifier =
    widthIn(minWidth, maxWidth).heightIn(minHeight, maxHeight)
@Stable fun Modifier.requiredWidth(width: Dp): Modifier = wire("""{"requiredWidth":${n(width)}}""")
@Stable fun Modifier.requiredHeight(height: Dp): Modifier = wire("""{"requiredHeight":${n(height)}}""")
@Stable fun Modifier.requiredSize(size: Dp): Modifier = requiredWidth(size).requiredHeight(size)
@Stable fun Modifier.requiredSize(width: Dp, height: Dp): Modifier = requiredWidth(width).requiredHeight(height)
@Stable fun Modifier.defaultMinSize(minWidth: Dp = Dp.Unspecified, minHeight: Dp = Dp.Unspecified): Modifier =
    wire("""{"defaultMinSize":[${n(minWidth)},${n(minHeight)}]}""")
@Stable fun Modifier.fillMaxWidth(fraction: Float = 1f): Modifier = wire("""{"fillMaxWidth":${jsonNum(fraction)}}""")
@Stable fun Modifier.fillMaxHeight(fraction: Float = 1f): Modifier = wire("""{"fillMaxHeight":${jsonNum(fraction)}}""")
@Stable fun Modifier.fillMaxSize(fraction: Float = 1f): Modifier = fillMaxWidth(fraction).fillMaxHeight(fraction)
@Stable fun Modifier.wrapContentSize(align: Alignment = Alignment.Center, unbounded: Boolean = false): Modifier =
    wire("""{"wrapContent":[${jsonNum(align.horizontalBias)},${jsonNum(align.verticalBias)}]}""")
@Stable fun Modifier.wrapContentWidth(align: Alignment.Horizontal = Alignment.CenterHorizontally, unbounded: Boolean = false): Modifier =
    wire("""{"wrapContent":[${jsonNum(align.bias)},null]}""")
@Stable fun Modifier.wrapContentHeight(align: Alignment.Vertical = Alignment.CenterVertically, unbounded: Boolean = false): Modifier =
    wire("""{"wrapContent":[null,${jsonNum(align.bias)}]}""")
@Stable fun Modifier.offset(x: Dp = 0.dp, y: Dp = 0.dp): Modifier = wire("""{"offset":[${n(x)},${n(y)}]}""")

/** System bar insets of the simulated device (dp), from `SIMCORE_INSETS`. */
@Immutable
class WindowInsets internal constructor(internal val top: Dp, internal val bottom: Dp) {
    fun asPaddingValues() = PaddingValues(0.dp, top, 0.dp, bottom)
    fun only(sides: WindowInsetsSides) = WindowInsets(if (sides.top) top else 0.dp, if (sides.bottom) bottom else 0.dp)
    fun exclude(insets: WindowInsets) = WindowInsets((top - insets.top).coerceAtLeastZero(), (bottom - insets.bottom).coerceAtLeastZero())
    fun union(insets: WindowInsets) = WindowInsets(if (top > insets.top) top else insets.top, if (bottom > insets.bottom) bottom else insets.bottom)
    fun add(insets: WindowInsets) = WindowInsets(top + insets.top, bottom + insets.bottom)
    private fun Dp.coerceAtLeastZero() = if (value < 0f) 0.dp else this

    companion object {
        val statusBars get() = WindowInsets(Guest.insetTop.dp, 0.dp)
        val navigationBars get() = WindowInsets(0.dp, Guest.insetBottom.dp)
        val systemBars get() = WindowInsets(Guest.insetTop.dp, Guest.insetBottom.dp)
        val safeDrawing get() = systemBars
        val safeContent get() = systemBars
        val safeGestures get() = systemBars
        val displayCutout get() = WindowInsets(Guest.insetTop.dp, 0.dp)
        val ime get() = WindowInsets(0.dp, 0.dp)
        val Zero = WindowInsets(0.dp, 0.dp)
    }
}

fun WindowInsets(left: Dp = 0.dp, top: Dp = 0.dp, right: Dp = 0.dp, bottom: Dp = 0.dp) = WindowInsets(top, bottom)

@Immutable
class WindowInsetsSides private constructor(internal val top: Boolean, internal val bottom: Boolean) {
    operator fun plus(other: WindowInsetsSides) = WindowInsetsSides(top || other.top, bottom || other.bottom)

    companion object {
        val Top = WindowInsetsSides(true, false)
        val Bottom = WindowInsetsSides(false, true)
        val Start = WindowInsetsSides(false, false)
        val End = WindowInsetsSides(false, false)
        val Left = Start
        val Right = End
        val Horizontal = WindowInsetsSides(false, false)
        val Vertical = WindowInsetsSides(true, true)
    }
}

fun Modifier.windowInsetsPadding(insets: WindowInsets): Modifier = padding(insets.asPaddingValues())
fun Modifier.statusBarsPadding(): Modifier = windowInsetsPadding(WindowInsets.statusBars)
fun Modifier.navigationBarsPadding(): Modifier = windowInsetsPadding(WindowInsets.navigationBars)
fun Modifier.systemBarsPadding(): Modifier = windowInsetsPadding(WindowInsets.systemBars)
fun Modifier.safeDrawingPadding(): Modifier = windowInsetsPadding(WindowInsets.safeDrawing)
fun Modifier.safeContentPadding(): Modifier = windowInsetsPadding(WindowInsets.safeContent)
fun Modifier.displayCutoutPadding(): Modifier = windowInsetsPadding(WindowInsets.displayCutout)
fun Modifier.imePadding(): Modifier = this
fun Modifier.consumeWindowInsets(insets: WindowInsets): Modifier = this
fun Modifier.consumeWindowInsets(paddingValues: PaddingValues): Modifier = this
fun Modifier.windowInsetsTopHeight(insets: WindowInsets): Modifier = height(insets.top)
fun Modifier.windowInsetsBottomHeight(insets: WindowInsets): Modifier = height(insets.bottom)
