package androidx.compose.foundation.layout

import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.Stable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.jsonNum
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.wire
import onesim.compose.Primitive

/** Main-axis placement (protocol section K `arrangement`). */
@Immutable
object Arrangement {
    @Stable
    interface Horizontal { val wire: String; val spacing: Dp get() = 0.dp }

    @Stable
    interface Vertical { val wire: String; val spacing: Dp get() = 0.dp }

    @Stable
    interface HorizontalOrVertical : Horizontal, Vertical {
        override val wire: String
        override val spacing: Dp get() = 0.dp
    }

    private class Named(override val wire: String) : HorizontalOrVertical

    private class Spaced(override val spacing: Dp, bias: Float) : HorizontalOrVertical {
        override val wire = """{"type":"spacedBy","space":${jsonNum(spacing.value)},"align":${jsonNum(bias)}}"""
    }

    val Start: Horizontal = Named("""{"type":"start"}""")
    val End: Horizontal = Named("""{"type":"end"}""")
    val Top: Vertical = Named("""{"type":"start"}""")
    val Bottom: Vertical = Named("""{"type":"end"}""")
    val Center: HorizontalOrVertical = Named("""{"type":"center"}""")
    val SpaceEvenly: HorizontalOrVertical = Named("""{"type":"spaceEvenly"}""")
    val SpaceBetween: HorizontalOrVertical = Named("""{"type":"spaceBetween"}""")
    val SpaceAround: HorizontalOrVertical = Named("""{"type":"spaceAround"}""")

    fun spacedBy(space: Dp): HorizontalOrVertical = Spaced(space, -1f)
    fun spacedBy(space: Dp, alignment: Alignment.Horizontal): Horizontal = Spaced(space, alignment.bias)
    fun spacedBy(space: Dp, alignment: Alignment.Vertical): Vertical = Spaced(space, alignment.bias)

    object Absolute {
        val Left: Horizontal = Start
        val Right: Horizontal = End
        val Center: Horizontal = Arrangement.Center
        val SpaceBetween: Horizontal = Arrangement.SpaceBetween
        val SpaceEvenly: Horizontal = Arrangement.SpaceEvenly
        val SpaceAround: Horizontal = Arrangement.SpaceAround
        fun spacedBy(space: Dp): HorizontalOrVertical = Arrangement.spacedBy(space)
    }
}

@LayoutScopeMarker
@Immutable
interface BoxScope {
    @Stable
    fun Modifier.align(alignment: Alignment): Modifier =
        wire("""{"align":[${jsonNum(alignment.horizontalBias)},${jsonNum(alignment.verticalBias)}]}""")

    // ponytail: matchParentSize is approximated by fillMaxSize (it ignores the Box's own size otherwise).
    @Stable
    fun Modifier.matchParentSize(): Modifier = fillMaxSize()
}

@LayoutScopeMarker
@Immutable
interface ColumnScope {
    @Stable
    fun Modifier.weight(weight: Float, fill: Boolean = true): Modifier {
        require(weight > 0f) { "invalid weight $weight; must be greater than zero" }
        return wire("""{"weight":${jsonNum(weight)},"fill":$fill}""")
    }

    @Stable
    fun Modifier.align(alignment: Alignment.Horizontal): Modifier = wire("""{"align":${jsonNum(alignment.bias)}}""")
}

@LayoutScopeMarker
@Immutable
interface RowScope {
    @Stable
    fun Modifier.weight(weight: Float, fill: Boolean = true): Modifier {
        require(weight > 0f) { "invalid weight $weight; must be greater than zero" }
        return wire("""{"weight":${jsonNum(weight)},"fill":$fill}""")
    }

    @Stable
    fun Modifier.align(alignment: Alignment.Vertical): Modifier = wire("""{"align":${jsonNum(alignment.bias)}}""")
}

@DslMarker
annotation class LayoutScopeMarker

internal object BoxScopeInstance : BoxScope
internal object ColumnScopeInstance : ColumnScope
internal object RowScopeInstance : RowScope

@Composable
fun Box(
    modifier: Modifier = Modifier,
    contentAlignment: Alignment = Alignment.TopStart,
    propagateMinConstraints: Boolean = false,
    content: @Composable BoxScope.() -> Unit,
) {
    val props = """"contentAlignment":[${jsonNum(contentAlignment.horizontalBias)},${jsonNum(contentAlignment.verticalBias)}]""" +
        if (propagateMinConstraints) ""","propagateMinConstraints":true""" else ""
    Primitive("Box", modifier, props) { BoxScopeInstance.content() }
}

@Composable
fun Box(modifier: Modifier) {
    Primitive("Box", modifier, "")
}

@Composable
fun Spacer(modifier: Modifier) {
    Primitive("Box", modifier, "")
}

@Composable
fun Column(
    modifier: Modifier = Modifier,
    verticalArrangement: Arrangement.Vertical = Arrangement.Top,
    horizontalAlignment: Alignment.Horizontal = Alignment.Start,
    content: @Composable ColumnScope.() -> Unit,
) {
    Primitive("Column", modifier, """"arrangement":${verticalArrangement.wire},"alignment":${jsonNum(horizontalAlignment.bias)}""") {
        ColumnScopeInstance.content()
    }
}

@Composable
fun Row(
    modifier: Modifier = Modifier,
    horizontalArrangement: Arrangement.Horizontal = Arrangement.Start,
    verticalAlignment: Alignment.Vertical = Alignment.Top,
    content: @Composable RowScope.() -> Unit,
) {
    Primitive("Row", modifier, """"arrangement":${horizontalArrangement.wire},"alignment":${jsonNum(verticalAlignment.bias)}""") {
        RowScopeInstance.content()
    }
}
