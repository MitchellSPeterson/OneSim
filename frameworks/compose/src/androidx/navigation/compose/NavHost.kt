package androidx.navigation.compose

import androidx.activity.BackEventCompat
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.AnimatedContentScope
import androidx.compose.animation.AnimatedContentTransitionScope
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.transitionFrame
import androidx.compose.animation.durationMs
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.SaveableStateHolder
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.saveable.LocalSaveableStateRegistry
import androidx.compose.runtime.saveable.SaveableStateRegistry
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.navigation.NavBackStackEntry
import androidx.navigation.NavDestination.Companion.hierarchy
import androidx.navigation.NavGraph
import androidx.navigation.NavGraphBuilder
import androidx.navigation.NavHostController
import androidx.navigation.Navigator
import androidx.navigation.NavDestination
import androidx.navigation.createGraph
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.Flow
import onesim.compose.Guest
import kotlin.math.roundToInt

private typealias EnterFn = AnimatedContentTransitionScope<NavBackStackEntry>.() -> EnterTransition
private typealias ExitFn = AnimatedContentTransitionScope<NavBackStackEntry>.() -> ExitTransition

/** NavHost's defaults: a 700 ms cross-fade both ways (`DefaultNavTransitions` on Android). */
object DefaultNavTransitions {
    val enterTransition: EnterFn = { fadeIn(animationSpec = tween(700)) }
    val exitTransition: ExitFn = { fadeOut(animationSpec = tween(700)) }
    fun popEnterTransition(enterTransition: EnterFn): EnterFn = enterTransition
    fun popExitTransition(exitTransition: ExitFn): ExitFn = exitTransition
    val predictivePopEnterTransition: AnimatedContentTransitionScope<NavBackStackEntry>.(swipeEdge: Int) -> EnterTransition =
        { _ -> fadeIn(animationSpec = tween(700)) }
    val predictivePopExitTransition: AnimatedContentTransitionScope<NavBackStackEntry>.(swipeEdge: Int) -> ExitTransition =
        { _ -> fadeOut(animationSpec = tween(700)) }
    val sizeTransform: (AnimatedContentTransitionScope<NavBackStackEntry>.() -> SizeTransform?)? = null
}

@Composable
fun rememberNavController(vararg navigators: Navigator<out NavDestination>): NavHostController = remember {
    NavHostController().apply {
        navigatorProvider.addNavigator(ComposeNavGraphNavigator(navigatorProvider))
        navigatorProvider.addNavigator(ComposeNavigator())
        navigatorProvider.addNavigator(DialogNavigator())
        navigators.forEach { navigatorProvider.addNavigator(it) }
    }
}

/** The current back stack entry as state (null before the graph is set). */
@Composable
fun androidx.navigation.NavController.currentBackStackEntryAsState(): State<NavBackStackEntry?> =
    currentBackStackEntryFlow.collectAsState(null)

/** Provides an entry's lifecycle owner and its saved UI state ([SaveableStateHolder]) to [content]. */
@Composable
fun NavBackStackEntry.LocalOwnersProvider(saveableStateHolder: SaveableStateHolder, content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalLifecycleOwner provides this) {
        saveableStateHolder.SaveableStateProvider(id, content)
    }
}

@Composable
fun NavHost(navController: NavHostController, startDestination: String, modifier: Modifier = Modifier, route: String? = null, builder: NavGraphBuilder.() -> Unit) {
    NavHost(navController, remember(route, startDestination, builder) { navController.createGraph(startDestination, route, builder) }, modifier)
}

@Composable
fun NavHost(
    navController: NavHostController,
    startDestination: String,
    modifier: Modifier = Modifier,
    contentAlignment: Alignment = Alignment.TopStart,
    route: String? = null,
    enterTransition: EnterFn = DefaultNavTransitions.enterTransition,
    exitTransition: ExitFn = DefaultNavTransitions.exitTransition,
    popEnterTransition: EnterFn = DefaultNavTransitions.popEnterTransition(enterTransition),
    popExitTransition: ExitFn = DefaultNavTransitions.popExitTransition(exitTransition),
    sizeTransform: (AnimatedContentTransitionScope<NavBackStackEntry>.() -> SizeTransform?)? = DefaultNavTransitions.sizeTransform,
    builder: NavGraphBuilder.() -> Unit,
) {
    NavHost(
        navController, remember(route, startDestination, builder) { navController.createGraph(startDestination, route, builder) },
        modifier, contentAlignment, enterTransition, exitTransition, popEnterTransition, popExitTransition,
        DefaultNavTransitions.predictivePopEnterTransition, DefaultNavTransitions.predictivePopExitTransition, sizeTransform,
    )
}

@Composable
fun NavHost(
    navController: NavHostController,
    startDestination: String,
    modifier: Modifier = Modifier,
    contentAlignment: Alignment = Alignment.TopStart,
    route: String? = null,
    enterTransition: EnterFn = DefaultNavTransitions.enterTransition,
    exitTransition: ExitFn = DefaultNavTransitions.exitTransition,
    popEnterTransition: EnterFn = DefaultNavTransitions.popEnterTransition(enterTransition),
    popExitTransition: ExitFn = DefaultNavTransitions.popExitTransition(exitTransition),
    predictivePopEnterTransition: AnimatedContentTransitionScope<NavBackStackEntry>.(Int) -> EnterTransition = DefaultNavTransitions.predictivePopEnterTransition,
    predictivePopExitTransition: AnimatedContentTransitionScope<NavBackStackEntry>.(Int) -> ExitTransition = DefaultNavTransitions.predictivePopExitTransition,
    sizeTransform: (AnimatedContentTransitionScope<NavBackStackEntry>.() -> SizeTransform?)? = DefaultNavTransitions.sizeTransform,
    builder: NavGraphBuilder.() -> Unit,
) {
    NavHost(
        navController, remember(route, startDestination, builder) { navController.createGraph(startDestination, route, builder) },
        modifier, contentAlignment, enterTransition, exitTransition, popEnterTransition, popExitTransition,
        predictivePopEnterTransition, predictivePopExitTransition, sizeTransform,
    )
}

/** A back stack change being animated: [from] leaves, [to] arrives; [seek] holds a predictive back gesture's progress. */
private class NavTransition(val from: NavBackStackEntry, val to: NavBackStackEntry, val pop: Boolean, val swipeEdge: Int?) {
    val elapsed = mutableFloatStateOf(0f)
    var seeking by mutableStateOf(false)
    var cancelled = false
}

/**
 * Shows the [navController]'s current destination; changes animate with the destinations' (or these) enter / exit
 * transitions, pops with the pop ones, and system back pops with predictive back (the pop transition follows the
 * swipe). Dialog destinations show over the content.
 */
@Composable
fun NavHost(
    navController: NavHostController,
    graph: NavGraph,
    modifier: Modifier = Modifier,
    contentAlignment: Alignment = Alignment.TopStart,
    enterTransition: EnterFn = DefaultNavTransitions.enterTransition,
    exitTransition: ExitFn = DefaultNavTransitions.exitTransition,
    popEnterTransition: EnterFn = DefaultNavTransitions.popEnterTransition(enterTransition),
    popExitTransition: ExitFn = DefaultNavTransitions.popExitTransition(exitTransition),
    predictivePopEnterTransition: AnimatedContentTransitionScope<NavBackStackEntry>.(Int) -> EnterTransition = DefaultNavTransitions.predictivePopEnterTransition,
    predictivePopExitTransition: AnimatedContentTransitionScope<NavBackStackEntry>.(Int) -> ExitTransition = DefaultNavTransitions.predictivePopExitTransition,
    sizeTransform: (AnimatedContentTransitionScope<NavBackStackEntry>.() -> SizeTransform?)? = DefaultNavTransitions.sizeTransform,
) {
    remember(graph) { navController.graph = graph }
    val holder = remember { EntryStateHolder() }
    val backStack by navController.currentBackStack.collectAsState()
    val screens = backStack.filter { it.destination.navigatorName == ComposeNavigator.NAME }
    val dialogs = backStack.filter { it.destination.navigatorName == DialogNavigator.NAME }
    val target = screens.lastOrNull()

    // Saved UI state of entries popped for good.
    val discarded = navController.discarded
    if (discarded.isNotEmpty()) {
        val ids = discarded.toList()
        LaunchedEffect(ids) {
            ids.forEach { holder.removeState(it) }
            navController.discarded.removeAll(ids)
        }
    }

    val shown = remember { mutableStateOf<NavBackStackEntry?>(null) }
    val transition = remember { mutableStateOf<NavTransition?>(null) }
    if (shown.value == null) shown.value = target
    val running = transition.value
    if (target != null && running == null && shown.value != null && shown.value!!.id != target.id && shown.value !== target) {
        transition.value = NavTransition(shown.value!!, target, navController.isPop.value, null)
    } else if (target != null && running != null && !running.seeking && running.to.id != target.id && running.from.id != target.id) {
        // Retargeted mid-animation: continue from the arriving destination.
        shown.value = running.to
        transition.value = NavTransition(running.to, target, navController.isPop.value, null)
    } else if (target == null) {
        shown.value = null
        transition.value = null
    }

    val t = transition.value
    fun scopeOf(tr: NavTransition) = object : AnimatedContentTransitionScope<NavBackStackEntry> {
        override val initialState get() = tr.from
        override val targetState get() = tr.to
    }
    fun enterOf(tr: NavTransition): EnterTransition {
        val scope = scopeOf(tr)
        if (tr.swipeEdge != null) return scope.predictivePopEnterTransition(tr.swipeEdge)
        for (d in tr.to.destination.hierarchy) {
            val f = when (d) {
                is ComposeNavigator.Destination -> if (tr.pop) d.popEnterTransition else d.enterTransition
                is ComposeNavGraphNavigator.ComposeNavGraph -> if (tr.pop) d.popEnterTransition else d.enterTransition
                else -> null
            }
            f?.invoke(scope)?.let { return it }
        }
        return if (tr.pop) popEnterTransition(scope) else enterTransition(scope)
    }
    fun exitOf(tr: NavTransition): ExitTransition {
        val scope = scopeOf(tr)
        if (tr.swipeEdge != null) return scope.predictivePopExitTransition(tr.swipeEdge)
        for (d in tr.from.destination.hierarchy) {
            val f = when (d) {
                is ComposeNavigator.Destination -> if (tr.pop) d.popExitTransition else d.exitTransition
                is ComposeNavGraphNavigator.ComposeNavGraph -> if (tr.pop) d.popExitTransition else d.exitTransition
                else -> null
            }
            f?.invoke(scope)?.let { return it }
        }
        return if (tr.pop) popExitTransition(scope) else exitTransition(scope)
    }
    val enter = t?.let { remember(it) { enterOf(it) } }
    val exit = t?.let { remember(it) { exitOf(it) } }
    val d = Guest.density
    val full = IntSize((Guest.screenWidth * d).roundToInt(), (Guest.screenHeight * d).roundToInt())

    // Run the animation (not while a back gesture holds it).
    if (t != null && !t.seeking) {
        LaunchedEffect(t, t.seeking) {
            var last = -1L
            while (true) {
                val now = withFrameNanos { it }
                if (last >= 0) {
                    val dt = (now - last) / 1_000_000f
                    t.elapsed.floatValue = if (t.cancelled) (t.elapsed.floatValue - dt).coerceAtLeast(0f) else t.elapsed.floatValue + dt
                }
                last = now
                val e = t.elapsed.floatValue
                if (t.cancelled && e <= 0f) break
                if (!t.cancelled && transitionFrame(enter!!.data, true, full, e).done && transitionFrame(exit!!.data, false, full, e).done) break
            }
            if (transition.value === t) {
                shown.value = if (t.cancelled) t.from else t.to
                transition.value = null
            }
        }
    }

    // System back pops, following a back swipe with the predictive pop transition.
    PredictiveBackHandler(enabled = screens.size > 1 && dialogs.isEmpty()) { progress: Flow<BackEventCompat> ->
        val current = screens.lastOrNull()
        val previous = screens.getOrNull(screens.size - 2)
        if (current == null || previous == null) {
            progress.collect {}
            navController.popBackStack()
            return@PredictiveBackHandler
        }
        var tr: NavTransition? = null
        try {
            progress.collect { e ->
                val g = tr ?: NavTransition(current, previous, true, e.swipeEdge).also {
                    it.seeking = true
                    tr = it
                    shown.value = current
                    transition.value = it
                }
                // Seek: the pop transition's duration scaled by the swipe's progress.
                g.elapsed.floatValue = e.progress * maxOf(enterOf(g).data.durationMs(), exitOf(g).data.durationMs())
            }
            val g = tr
            if (g != null) {
                g.seeking = false
                navController.isPop.value = true
            }
            navController.popBackStack()
        } catch (c: CancellationException) {
            tr?.let {
                it.cancelled = true
                it.seeking = false
            }
            throw c
        }
    }

    val tNow = t
    Box(modifier, contentAlignment = contentAlignment) {
        val layers: List<Pair<NavBackStackEntry, Boolean>> = when {
            tNow == null -> listOfNotNull(shown.value?.let { it to true })
            // Pops keep the leaving destination on top, pushes put the arriving one there.
            tNow.pop -> listOf(tNow.to to true, tNow.from to false)
            else -> listOf(tNow.from to false, tNow.to to true)
        }
        for ((entry, entering) in layers) {
            key(entry.id) {
                val frame = if (tNow == null) null else transitionFrame(
                    if (entering) enter!!.data else exit!!.data, entering, full, tNow.elapsed.floatValue,
                )
                val m = if (frame == null) Modifier else Modifier.alpha(frame.alpha).offset { IntOffset(frame.offset.x, frame.offset.y) }
                Box(m) {
                    entry.LocalOwnersProvider(holder) {
                        val dest = entry.destination as? ComposeNavigator.Destination
                        dest?.content?.invoke(ContentScope, entry)
                    }
                }
            }
        }
    }
    DialogHost(dialogs, holder, navController)
}

private object ContentScope : AnimatedContentScope

/**
 * Keeps each back stack entry's `rememberSaveable` state while it is off screen (on the back stack or in a saved back
 * stack), like Compose's `SaveableStateHolder`, on a [SaveableStateRegistry] per entry.
 */
private class EntryStateHolder : SaveableStateHolder {
    private val saved = HashMap<Any, Map<String, List<Any?>>>()
    private val live = HashMap<Any, SaveableStateRegistry>()

    @Composable
    override fun SaveableStateProvider(key: Any, content: @Composable () -> Unit) {
        androidx.compose.runtime.key(key) {
            val registry = remember { SaveableStateRegistry(saved.remove(key)) { true } }
            CompositionLocalProvider(LocalSaveableStateRegistry provides registry, content = content)
            DisposableEffect(key) {
                live[key] = registry
                onDispose {
                    if (live.remove(key) === registry) saved[key] = registry.performSave()
                }
            }
        }
    }

    override fun removeState(key: Any) {
        saved.remove(key)
        live.remove(key)
    }
}


@Composable
private fun DialogHost(dialogs: List<NavBackStackEntry>, holder: SaveableStateHolder, navController: NavHostController) {
    for (entry in dialogs) {
        val dest = entry.destination as? DialogNavigator.Destination ?: continue
        key(entry.id) {
            val props = dest.dialogProperties
            Box(
                Modifier.fillMaxSize().background(Color(0f, 0f, 0f, 0.6f)).then(
                    if (props.dismissOnClickOutside) {
                        Modifier.clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { navController.popBackStack() }
                    } else {
                        Modifier
                    },
                ),
                contentAlignment = Alignment.Center,
            ) {
                Box(
                    (if (props.usePlatformDefaultWidth) Modifier.widthIn(max = (Guest.screenWidth * 0.9f).dp) else Modifier)
                        .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {},
                ) {
                    entry.LocalOwnersProvider(holder) { dest.content(entry) }
                }
            }
            // The dialog window takes system back; it closes the dialog only with dismissOnBackPress.
            androidx.activity.compose.BackHandler { if (props.dismissOnBackPress) navController.popBackStack() }
        }
    }
}
