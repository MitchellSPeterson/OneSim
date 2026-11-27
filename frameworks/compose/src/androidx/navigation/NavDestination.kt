package androidx.navigation

import android.os.Bundle
import kotlin.jvm.JvmStatic

/**
 * A screen in a navigation graph, identified by its [route] (`"profile/{id}?tab={tab}"`: path arguments in braces,
 * optional query arguments). [id] is derived from the route as in AndroidX.
 */
open class NavDestination(val navigatorName: String) {
    var parent: NavGraph? = null
        internal set
    var label: CharSequence? = null
    private val argumentsMap = LinkedHashMap<String, NavArgument>()
    private val links = ArrayList<NavDeepLink>()
    private var routePattern: RoutePattern? = null

    var id: Int = 0

    var route: String? = null
        set(value) {
            require(value == null || value.isNotBlank()) { "Cannot have an empty route" }
            field = value
            routePattern = value?.let { RoutePattern(it) }
            id = value?.let { createRoute(it).hashCode() } ?: 0
        }

    val arguments: Map<String, NavArgument> get() = argumentsMap

    fun addArgument(argumentName: String, argument: NavArgument) {
        argumentsMap[argumentName] = argument
    }

    fun removeArgument(argumentName: String) {
        argumentsMap.remove(argumentName)
    }

    fun addDeepLink(navDeepLink: NavDeepLink) {
        links.add(navDeepLink)
    }

    fun addDeepLink(uriPattern: String) = addDeepLink(NavDeepLink.Builder.fromUriPattern(uriPattern).build())

    internal val deepLinks: List<NavDeepLink> get() = links

    /** Arguments with defaults, then [args] on top. */
    fun addInDefaultArgs(args: Bundle?): Bundle? {
        if (args == null && argumentsMap.values.none { it.isDefaultValuePresent }) return null
        val out = Bundle()
        for ((name, arg) in argumentsMap) arg.putDefaultValue(name, out)
        if (args != null) out.putAll(args)
        return out
    }

    /** How well [route] (which matches) fits: exact routes first, then the number of arguments it gives. */
    internal fun routeRank(route: String): Int {
        val pattern = routePattern ?: return 0
        return (if (pattern.exact) 1 shl 16 else 0) + (pattern.match(route)?.count { it.value != null } ?: 0)
    }

    /** The arguments [route] gives this destination, or null if it does not match the route pattern. */
    internal fun matchRoute(route: String): Bundle? {
        val pattern = routePattern ?: return null
        val values = pattern.match(route) ?: return null
        val bundle = Bundle()
        for ((name, arg) in argumentsMap) arg.putDefaultValue(name, bundle)
        for ((name, raw) in values) {
            val arg = argumentsMap[name]
            try {
                if (arg != null) {
                    if (raw == null) {
                        if (!arg.isNullable && !arg.isDefaultValuePresent) return null
                        if (arg.isNullable) bundle.putRaw(name, null)
                    } else {
                        arg.type.parseAndPut(bundle, name, raw)
                    }
                } else if (raw != null) {
                    bundle.putString(name, raw)
                }
            } catch (_: IllegalArgumentException) {
                return null
            }
        }
        // Required (non-null, no default) path arguments must be present.
        for ((name, arg) in argumentsMap) {
            if (!bundle.containsKey(name) && !arg.isNullable && !arg.isDefaultValuePresent && name in pattern.pathArgs) return null
        }
        return bundle
    }

    /** Whether [route] (with [arguments]) is this destination's route. */
    fun hasRoute(route: String, arguments: Bundle?): Boolean {
        if (this.route == route) return true
        val matched = matchRoute(route) ?: return false
        if (arguments == null) return true
        return matched.keySet().all { k -> !arguments.containsKey(k) || arguments.raw(k) == matched.raw(k) }
    }

    open fun hasDeepLink(deepLink: String): Boolean = links.any { it.uriPattern == deepLink }

    override fun toString(): String = buildString {
        append(this@NavDestination::class.simpleName ?: "NavDestination")
        append("(0x").append(id.toString(16)).append(")")
        route?.let { append(" route=").append(it) }
        label?.let { append(" label=").append(it) }
    }

    override fun equals(other: Any?) = other is NavDestination && other::class == this::class && route == other.route && id == other.id &&
        argumentsMap == other.argumentsMap
    override fun hashCode() = (route.hashCode() * 31 + id) * 31 + argumentsMap.keys.hashCode()

    companion object {
        /** This destination, then its parent graphs out to the root. */
        @JvmStatic
        val NavDestination.hierarchy: Sequence<NavDestination>
            get() = generateSequence(this) { it.parent }

        @JvmStatic
        fun createRoute(route: String?): String = if (route != null) "android-app://androidx.navigation/$route" else ""

        @JvmStatic
        fun getDisplayName(context: Any?, id: Int): String = id.toString()
    }
}

/** A route pattern: literal path segments and `{name}` arguments, then optional `name={name}` query arguments. */
internal class RoutePattern(pattern: String) {
    private val path: List<String>
    private val query: List<Pair<String, String?>>
    val pathArgs: Set<String>

    init {
        val q = pattern.indexOf('?')
        val p = if (q >= 0) pattern.substring(0, q) else pattern
        path = p.split('/')
        query = if (q < 0) emptyList() else pattern.substring(q + 1).split('&').filter { it.isNotEmpty() }.map { part ->
            val eq = part.indexOf('=')
            if (eq < 0) part to null else part.substring(0, eq) to argName(part.substring(eq + 1))
        }
        pathArgs = path.mapNotNull { argName(it) }.toSet()
    }

    /** No arguments in the pattern (AndroidX's exact deep link). */
    val exact: Boolean get() = pathArgs.isEmpty() && query.none { it.second != null }

    private fun argName(s: String): String? = if (s.length > 2 && s.startsWith("{") && s.endsWith("}")) s.substring(1, s.length - 1) else null

    /** Argument values (null = given as absent) for a concrete [route], or null when it does not match. */
    fun match(route: String): Map<String, String?>? {
        val q = route.indexOf('?')
        val p = if (q >= 0) route.substring(0, q) else route
        val segments = p.split('/')
        if (segments.size != path.size) return null
        val out = LinkedHashMap<String, String?>()
        for ((pat, seg) in path.zip(segments)) {
            val name = argName(pat)
            if (name != null) {
                if (seg.isEmpty()) return null
                out[name] = decode(seg)
            } else if (pat != seg) {
                return null
            }
        }
        val given = if (q < 0) emptyMap() else route.substring(q + 1).split('&').filter { it.isNotEmpty() }.associate { part ->
            val eq = part.indexOf('=')
            if (eq < 0) decode(part) to "" else decode(part.substring(0, eq)) to decode(part.substring(eq + 1))
        }
        for ((key, arg) in query) {
            if (arg == null) continue
            if (key in given) out[arg] = given[key] else out[arg] = null
        }
        for ((key, value) in given) if (query.none { it.first == key }) out[key] = value
        return out
    }

    private fun decode(s: String): String {
        if ('%' !in s && '+' !in s) return s
        val bytes = ArrayList<Byte>()
        var i = 0
        while (i < s.length) {
            val c = s[i]
            if (c == '%' && i + 2 < s.length) {
                val hex = s.substring(i + 1, i + 3).toIntOrNull(16)
                if (hex != null) {
                    bytes.add(hex.toByte())
                    i += 3
                    continue
                }
            }
            c.toString().encodeToByteArray().forEach { bytes.add(it) }
            i++
        }
        return bytes.toByteArray().decodeToString()
    }
}

/** A graph of destinations with a start destination; graphs nest. */
open class NavGraph(navGraphNavigator: Navigator<out NavGraph>) : NavDestination(navGraphNavigator.name), Iterable<NavDestination> {
    private val nodes = LinkedHashMap<Int, NavDestination>()

    var startDestinationRoute: String? = null
        private set
    var startDestinationId: Int = 0
        private set

    val nodeCount: Int get() = nodes.size

    fun addDestination(node: NavDestination) {
        require(node.route != null || node.id != 0) { "Destinations must have a route or an id" }
        require(node.id != id || node.route != route) { "Destination $node cannot have the same id as graph $this" }
        node.parent?.nodes?.remove(node.id)
        node.parent = this
        nodes[node.id] = node
    }

    fun addDestinations(nodes: Collection<NavDestination>) = nodes.forEach(::addDestination)

    fun addDestinations(vararg nodes: NavDestination) = nodes.forEach(::addDestination)

    fun remove(node: NavDestination) {
        if (nodes.remove(node.id) != null) node.parent = null
    }

    fun clear() = nodes.values.toList().forEach(::remove)

    fun setStartDestination(startDestRoute: String) {
        startDestinationRoute = startDestRoute
        startDestinationId = createRoute(startDestRoute).hashCode()
    }

    fun setStartDestination(startDestId: Int) {
        startDestinationId = startDestId
        startDestinationRoute = null
    }

    /** The direct child with [resId], or (searching parents) any destination with it. */
    fun findNode(resId: Int): NavDestination? = findNode(resId, true)

    internal fun findNode(resId: Int, searchParents: Boolean): NavDestination? =
        nodes[resId] ?: if (searchParents) parent?.findNode(resId) else null

    fun findNode(route: String?): NavDestination? {
        if (route.isNullOrBlank()) return null
        return nodes[createRoute(route).hashCode()] ?: nodes.values.firstOrNull { it.matchRoute(route) != null } ?: parent?.findNode(route)
    }

    /**
     * The destination in this graph or its subgraphs that best matches [route], as AndroidX's
     * `matchRouteComprehensive`: a route without arguments beats a pattern, then more arguments taken from [route]
     * win; on a tie the first found (this graph's destinations, then its subgraphs, depth first).
     */
    internal fun findDeep(route: String): Pair<NavDestination, Bundle>? {
        var best: Pair<NavDestination, Bundle>? = null
        var bestRank = -1
        fun visit(g: NavGraph) {
            for (n in g.nodes.values) {
                val args = n.matchRoute(route) ?: continue
                val rank = n.routeRank(route)
                if (rank > bestRank) {
                    best = n to args
                    bestRank = rank
                }
            }
            for (n in g.nodes.values) if (n is NavGraph) visit(n)
        }
        visit(this)
        return best
    }

    internal fun findDeep(id: Int): NavDestination? {
        nodes[id]?.let { return it }
        for (n in nodes.values) if (n is NavGraph) n.findDeep(id)?.let { return it }
        return null
    }

    override fun iterator(): MutableIterator<NavDestination> = nodes.values.iterator()

    override fun toString(): String = super.toString() + " startDestination=${startDestinationRoute ?: startDestinationId}"

    override fun equals(other: Any?) = other is NavGraph && super.equals(other) && nodes == other.nodes && startDestinationId == other.startDestinationId
    override fun hashCode() = super.hashCode() * 31 + startDestinationId

    companion object {
        /** The innermost start destination of this graph (following nested graphs' start destinations). */
        @JvmStatic
        fun NavGraph.findStartDestination(): NavDestination {
            var d: NavDestination = this
            while (d is NavGraph) d = d.findNode(d.startDestinationId, false) ?: break
            return d
        }
    }
}

/** Something that shows destinations of one kind (composables, dialogs, graphs). */
abstract class Navigator<D : NavDestination> {
    open val name: String get() = this::class.simpleName ?: "navigator"
    abstract fun createDestination(): D

    /** Extra data for a navigation (shared element views on Android); OneSim ignores it. */
    interface Extras
}

/** The navigators of a [NavController], by name. */
open class NavigatorProvider {
    private val navigators = LinkedHashMap<String, Navigator<out NavDestination>>()

    fun <T : Navigator<*>> getNavigator(name: String): T {
        @Suppress("UNCHECKED_CAST")
        return navigators[name] as? T ?: throw IllegalStateException("Could not find Navigator with name \"$name\". You must call NavController.addNavigator() for each navigation type.")
    }

    fun addNavigator(navigator: Navigator<out NavDestination>): Navigator<out NavDestination>? = navigators.put(navigator.name, navigator)

    fun addNavigator(name: String, navigator: Navigator<out NavDestination>): Navigator<out NavDestination>? = navigators.put(name, navigator)
}

inline operator fun <reified T : Navigator<out NavDestination>> NavigatorProvider.get(name: String): T = getNavigator(name)

operator fun NavigatorProvider.plusAssign(navigator: Navigator<out NavDestination>) {
    addNavigator(navigator)
}

/** Navigator of nested [NavGraph]s. */
open class NavGraphNavigator(private val navigatorProvider: NavigatorProvider) : Navigator<NavGraph>() {
    override val name: String get() = "navigation"
    override fun createDestination(): NavGraph = NavGraph(this)
}
