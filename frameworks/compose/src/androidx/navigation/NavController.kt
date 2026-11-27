package androidx.navigation

import android.os.Bundle
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.lifecycle.Lifecycle
import androidx.navigation.NavDestination.Companion.hierarchy
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.distinctUntilChanged
import onesim.compose.Guest

/**
 * Owns the back stack of a navigation graph: [navigate] pushes destinations by route (with `popUpTo`, single top and
 * saved / restored back stacks), [popBackStack] / [navigateUp] pop them. Graph destinations are kept on the back stack
 * under their children, as in AndroidX.
 */
open class NavController {
    val navigatorProvider: NavigatorProvider = NavigatorProvider().also { it.addNavigator(NavGraphNavigator(it)) }

    private val backQueue = ArrayList<NavBackStackEntry>()
    private val backStackFlow = MutableStateFlow<List<NavBackStackEntry>>(emptyList())
    private val visibleFlow = MutableStateFlow<List<NavBackStackEntry>>(emptyList())
    private val currentEntryFlow = MutableStateFlow<NavBackStackEntry?>(null)
    private val listeners = ArrayList<OnDestinationChangedListener>()
    private var lastDispatched: Pair<NavDestination, Bundle?>? = null
    private var nextId = 0

    /** Saved back stacks (`popUpTo(saveState = true)`): stack id -> entries, and destination id -> stack id. */
    private val savedStacks = LinkedHashMap<String, List<NavBackStackEntry>>()
    /** Destination id -> saved back stack to restore for it (null: navigate there normally), as in AndroidX. */
    private val backStackMap = LinkedHashMap<Int, String?>()

    /** Whether the latest back stack change was a pop (NavHost picks pop transitions). */
    internal val isPop = mutableStateOf(false)
    /** Entries popped for good, whose saved UI state the NavHost drops. */
    internal val discarded = mutableStateListOf<String>()

    private var graphValue: NavGraph? = null

    /** The graph; setting it shows its start destination. */
    open var graph: NavGraph
        get() = checkNotNull(graphValue) { "You must call setGraph() before calling getGraph()" }
        set(value) = setGraph(value, null)

    open fun setGraph(graph: NavGraph, startDestinationArgs: Bundle?) {
        val old = graphValue
        graphValue = graph
        if (old != null && old == graph && backQueue.isNotEmpty()) {
            // The same graph rebuilt (a recomposed NavHost): keep the back stack, point entries at the new destinations.
            for (e in backQueue) {
                val d = if (e.destination.id == graph.id) graph else graph.findDeep(e.destination.id)
                if (d != null) e.destination = d
            }
            dispatch()
            return
        }
        for (e in backQueue.toList()) discard(e)
        backQueue.clear()
        savedStacks.clear()
        backStackMap.clear()
        backQueue.add(newEntry(graph, startDestinationArgs))
        var start: NavDestination = graph
        while (start is NavGraph) {
            val next = start.findNode(start.startDestinationId, false)
                ?: throw IllegalArgumentException("navigation destination ${start.startDestinationRoute ?: start.startDestinationId} is not a direct child of this NavGraph")
            if (next is NavGraph) backQueue.add(newEntry(next, startDestinationArgs))
            start = next
        }
        backQueue.add(newEntry(start, start.addInDefaultArgs(startDestinationArgs)))
        isPop.value = false
        dispatch()
    }

    private fun newEntry(destination: NavDestination, args: Bundle?) = NavBackStackEntry(destination, args, "entry-${nextId++}")

    val currentBackStack: StateFlow<List<NavBackStackEntry>> get() = backStackFlow.asStateFlow()
    val visibleEntries: StateFlow<List<NavBackStackEntry>> get() = visibleFlow.asStateFlow()

    open val currentBackStackEntry: NavBackStackEntry? get() = backQueue.lastOrNull()

    open val previousBackStackEntry: NavBackStackEntry?
        get() = backQueue.asReversed().asSequence().filter { it.destination !is NavGraph }.drop(1).firstOrNull()

    open val currentDestination: NavDestination? get() = currentBackStackEntry?.destination

    val currentBackStackEntryFlow: Flow<NavBackStackEntry> get() = currentEntryFlow.filterNotNull().distinctUntilChanged()

    fun interface OnDestinationChangedListener {
        fun onDestinationChanged(controller: NavController, destination: NavDestination, arguments: Bundle?)
    }

    open fun addOnDestinationChangedListener(listener: OnDestinationChangedListener) {
        listeners.add(listener)
        currentBackStackEntry?.let { listener.onDestinationChanged(this, it.destination, it.arguments) }
    }

    open fun removeOnDestinationChangedListener(listener: OnDestinationChangedListener) {
        listeners.remove(listener)
    }

    // ---- navigate ------------------------------------------------------------------------------------------------

    open fun navigate(route: String, builder: NavOptionsBuilder.() -> Unit) = navigate(route, navOptions(builder))

    open fun navigate(route: String, navOptions: NavOptions? = null, navigatorExtras: Navigator.Extras? = null) {
        val g = graph
        val match = (if (g.route == route) g to (g.addInDefaultArgs(null) ?: Bundle()) else null)
            ?: g.findDeep(route)
            ?: throw IllegalArgumentException("Navigation destination that matches route $route cannot be found in the navigation graph $g")
        navigateTo(match.first, match.second, navOptions)
    }

    open fun navigate(resId: Int, args: Bundle? = null, navOptions: NavOptions? = null, navigatorExtras: Navigator.Extras? = null) {
        val g = graph
        val node = (if (g.id == resId) g else g.findDeep(resId))
            ?: throw IllegalArgumentException("Navigation action/destination $resId cannot be found from the current destination $currentDestination")
        navigateTo(node, node.addInDefaultArgs(args), navOptions)
    }

    open fun navigate(directions: NavDirections, navOptions: NavOptions? = null) = navigate(directions.actionId, directions.arguments, navOptions)

    private fun navigateTo(node: NavDestination, args: Bundle?, options: NavOptions?) {
        if (options != null) {
            when {
                options.popUpToRoute != null -> popTo(options.popUpToRoute, options.isPopUpToInclusive(), options.shouldPopUpToSaveState())
                options.popUpToId != -1 -> popTo(options.popUpToId, options.isPopUpToInclusive(), options.shouldPopUpToSaveState())
            }
        }
        if (options?.shouldRestoreState() == true && restore(node.id)) {
            isPop.value = false
            dispatch()
            return
        }
        // Graphs go to their start destination, which gets its own argument defaults (NavGraphNavigator).
        var target = node
        while (target is NavGraph) {
            target = target.findNode(target.startDestinationId, false)
                ?: throw IllegalArgumentException("navigation destination ${target.startDestinationRoute} is not a direct child of this NavGraph")
        }
        val targetArgs = if (target === node) args else target.addInDefaultArgs(args)
        val top = backQueue.lastOrNull()
        if (options?.shouldLaunchSingleTop() == true && top != null && top.destination.id == target.id && top.destination !is NavGraph) {
            top.replaceArguments(targetArgs)
            isPop.value = false
            dispatch()
            return
        }
        // Parent graphs not already enclosing the top of the back stack, outermost first.
        val parents = target.hierarchy.drop(1).filterIsInstance<NavGraph>().toList().asReversed()
        for (p in parents) if (!encloses(p)) backQueue.add(newEntry(p, args))
        backQueue.add(newEntry(target, targetArgs))
        isPop.value = false
        dispatch()
    }

    /** Whether an entry of graph [g] is on the back stack with only its own destinations above it. */
    private fun encloses(g: NavGraph): Boolean {
        val i = backQueue.indexOfLast { it.destination.id == g.id && it.destination is NavGraph }
        if (i < 0) return false
        return backQueue.subList(i + 1, backQueue.size).all { e -> e.destination.hierarchy.any { it.id == g.id } }
    }

    // ---- pop -----------------------------------------------------------------------------------------------------

    /** Pops the current destination; false if the back stack has none. */
    open fun popBackStack(): Boolean {
        val top = backQueue.lastOrNull { it.destination !is NavGraph } ?: return false
        return popBackStack(top.destination.id, inclusive = true, saveState = false)
    }

    open fun popBackStack(route: String, inclusive: Boolean, saveState: Boolean = false): Boolean {
        val popped = popTo(route, inclusive, saveState)
        if (popped) {
            isPop.value = true
            dispatch()
        }
        return popped
    }

    open fun popBackStack(destinationId: Int, inclusive: Boolean, saveState: Boolean = false): Boolean {
        val popped = popTo(destinationId, inclusive, saveState)
        if (popped) {
            isPop.value = true
            dispatch()
        }
        return popped
    }

    private fun popTo(route: String, inclusive: Boolean, saveState: Boolean): Boolean {
        val i = backQueue.indexOfLast { it.destination.route == route || it.destination.hasRoute(route, it.arguments) }
        if (i < 0) {
            Guest.log("NavController: ignoring popBackStack to route $route as it was not found on the current back stack")
            return false
        }
        return popFrom(if (inclusive) i else i + 1, saveState, if (inclusive) null else backQueue[i].destination)
    }

    private fun popTo(id: Int, inclusive: Boolean, saveState: Boolean): Boolean {
        val i = backQueue.indexOfLast { it.destination.id == id }
        if (i < 0) {
            Guest.log("NavController: ignoring popBackStack to destination $id as it was not found on the current back stack")
            return false
        }
        return popFrom(if (inclusive) i else i + 1, saveState, if (inclusive) null else backQueue[i].destination)
    }

    /**
     * Pops every entry from index [from] up; with [saveState] they are kept to restore later, for the first popped
     * destination and for [kept] (a non-inclusive popUpTo destination, left on the stack), each with the parent graphs
     * it starts (as AndroidX's popBackStackInternal). [kept] is mapped even when nothing is popped: to no saved stack.
     */
    private fun popFrom(from: Int, saveState: Boolean, kept: NavDestination? = null): Boolean {
        if (from >= backQueue.size) {
            if (saveState && kept != null) mapSaved(kept, null)
            return false
        }
        var start = from
        // Graphs left without destinations above them go too; with no destination left, the whole back stack goes
        // (as in AndroidX: the NavHost shows nothing).
        while (start > 0 && backQueue[start - 1].destination is NavGraph) start--
        if (start > 0 && backQueue.subList(0, start).all { it.destination is NavGraph }) start = 0
        val popped = backQueue.subList(start, backQueue.size).toList()
        repeat(popped.size) { backQueue.removeAt(backQueue.size - 1) }
        val stackId = if (saveState) "stack-${nextId++}" else null
        if (stackId != null) {
            kept?.let { mapSaved(it, stackId) }
            mapSaved(popped.first().destination, stackId)
        }
        if (stackId != null && stackId in backStackMap.values) {
            savedStacks[stackId] = popped
            for (e in popped) e.maxLifecycle = Lifecycle.State.CREATED
        } else {
            popped.forEach(::discard)
        }
        return true
    }

    /** Maps [d], and each parent graph whose start destination it is, to [stackId], stopping at one already mapped. */
    private fun mapSaved(d: NavDestination, stackId: String?) {
        var cur: NavDestination? = d
        while (cur != null && !backStackMap.containsKey(cur.id)) {
            backStackMap[cur.id] = stackId
            val parent = cur.parent
            cur = if (parent != null && parent.startDestinationId == cur.id) parent else null
        }
    }

    private fun restore(destinationId: Int): Boolean {
        if (!backStackMap.containsKey(destinationId)) return false
        val stackId = backStackMap[destinationId]
        backStackMap.entries.removeAll { it.value == stackId }
        val entries = savedStacks.remove(stackId ?: return false) ?: return false
        for (e in entries) {
            if (e.destination is NavGraph && backQueue.any { it.destination.id == e.destination.id }) continue
            backQueue.add(e)
        }
        return true
    }

    /** Drops a saved back stack (`popUpTo(saveState = true)`) of the destination [route]. */
    open fun clearBackStack(route: String): Boolean {
        val node = graphValue?.findDeep(route)?.first ?: return false
        return clearBackStack(node.id)
    }

    open fun clearBackStack(destinationId: Int): Boolean {
        if (!backStackMap.containsKey(destinationId)) return false
        val stackId = backStackMap[destinationId]
        backStackMap.entries.removeAll { it.value == stackId }
        val entries = savedStacks.remove(stackId ?: return false) ?: return false
        entries.forEach(::discard)
        return true
    }

    /** Up: pops the current destination unless it is the only one (the activity would finish instead). */
    open fun navigateUp(): Boolean {
        if (backQueue.count { it.destination !is NavGraph } <= 1) return false
        return popBackStack()
    }

    fun getBackStackEntry(route: String): NavBackStackEntry =
        backQueue.lastOrNull { it.destination.route == route || it.destination.hasRoute(route, it.arguments) }
            ?: throw IllegalArgumentException("No destination with route $route is on the NavController's back stack. The current destination is $currentDestination")

    fun getBackStackEntry(destinationId: Int): NavBackStackEntry =
        backQueue.lastOrNull { it.destination.id == destinationId }
            ?: throw IllegalArgumentException("No destination with ID $destinationId is on the NavController's back stack. The current destination is $currentDestination")

    private fun discard(e: NavBackStackEntry) {
        e.maxLifecycle = Lifecycle.State.DESTROYED
        discarded.add(e.id)
    }

    private fun dispatch() {
        val list = backQueue.toList()
        val top = list.lastOrNull { it.destination !is NavGraph }
        // RESUMED on top (and the graphs holding it), STARTED under a dialog, CREATED elsewhere.
        val dialogTop = top?.destination?.navigatorName == "dialog"
        val underDialog = if (dialogTop) list.lastOrNull { it.destination !is NavGraph && it !== top } else null
        val holding = top?.destination?.hierarchy?.map { it.id }?.toSet().orEmpty()
        for (e in list) {
            e.maxLifecycle = when {
                e === top -> Lifecycle.State.RESUMED
                e === underDialog -> Lifecycle.State.STARTED
                e.destination is NavGraph && e.destination.id in holding -> Lifecycle.State.RESUMED
                else -> Lifecycle.State.CREATED
            }
        }
        backStackFlow.value = list.toList()
        visibleFlow.value = list.filter { it.maxLifecycle.isAtLeast(Lifecycle.State.STARTED) && it.destination !is NavGraph }
        currentEntryFlow.value = list.lastOrNull()
        if (top != null) {
            val now = top.destination to top.arguments
            // The same destination (by id) with the same arguments is no change, even from a rebuilt graph.
            if (lastDispatched?.first?.id != now.first.id || lastDispatched?.second != now.second) {
                lastDispatched = now
                for (l in listeners.toList()) l.onDestinationChanged(this, top.destination, top.arguments)
            }
        }
    }
}

/** A navigation action with its destination id and arguments (Safe Args). */
interface NavDirections {
    val actionId: Int
    val arguments: Bundle
}

/** Builds a graph for this controller's navigators: [startDestination] route, the graph's own [route]. */
inline fun NavController.createGraph(startDestination: String, route: String? = null, builder: NavGraphBuilder.() -> Unit): NavGraph =
    NavGraphBuilder(navigatorProvider, startDestination, route).apply(builder).build()

/** The controller of a NavHost; OneSim's activity wiring (lifecycle, back dispatcher) is done by the NavHost. */
open class NavHostController : NavController() {
    fun setLifecycleOwner(owner: androidx.lifecycle.LifecycleOwner) {}
    fun setOnBackPressedDispatcher(dispatcher: androidx.activity.OnBackPressedDispatcher) {}
    fun enableOnBackPressed(enabled: Boolean) {}
}

/** Builds a [NavGraph]: destinations added with `composable`, `dialog`, `navigation`. */
open class NavGraphBuilder(val provider: NavigatorProvider, private val startDestination: String, val route: String?) {
    private val destinations = ArrayList<NavDestination>()

    fun addDestination(destination: NavDestination) {
        destinations.add(destination)
    }

    operator fun NavDestination.unaryPlus() = addDestination(this)

    open fun build(): NavGraph {
        val graph = provider.getNavigator<NavGraphNavigator>("navigation").createDestination()
        return finish(graph)
    }

    internal fun finish(graph: NavGraph): NavGraph {
        if (route != null) graph.route = route
        graph.addDestinations(destinations)
        graph.setStartDestination(startDestination)
        return graph
    }
}

/** A nested graph (`navigation(startDestination, route) { }`). */
fun NavGraphBuilder.navigation(startDestination: String, route: String, builder: NavGraphBuilder.() -> Unit) {
    addDestination(NavGraphBuilder(provider, startDestination, route).apply(builder).build())
}
