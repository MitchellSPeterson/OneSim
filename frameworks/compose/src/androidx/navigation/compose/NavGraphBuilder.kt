package androidx.navigation.compose

import androidx.compose.animation.AnimatedContentScope
import androidx.compose.animation.AnimatedContentTransitionScope
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.SizeTransform
import androidx.compose.runtime.Composable
import androidx.compose.ui.window.DialogProperties
import androidx.navigation.NamedNavArgument
import androidx.navigation.NavBackStackEntry
import androidx.navigation.NavDeepLink
import androidx.navigation.NavDestination
import androidx.navigation.NavGraph
import androidx.navigation.NavGraphBuilder
import androidx.navigation.NavGraphNavigator
import androidx.navigation.Navigator
import androidx.navigation.NavigatorProvider

private typealias Enter = AnimatedContentTransitionScope<NavBackStackEntry>.() -> EnterTransition?
private typealias Exit = AnimatedContentTransitionScope<NavBackStackEntry>.() -> ExitTransition?

/** Shows [Destination]s in a NavHost. */
class ComposeNavigator : Navigator<ComposeNavigator.Destination>() {
    override val name: String get() = NAME

    override fun createDestination(): Destination = Destination(this) {}

    /** A composable destination; null transitions fall back to its graphs' and then the NavHost's. */
    class Destination(
        navigator: ComposeNavigator,
        internal val content: @Composable AnimatedContentScope.(NavBackStackEntry) -> Unit,
    ) : NavDestination(NAME) {
        var enterTransition: Enter? = null
        var exitTransition: Exit? = null
        var popEnterTransition: Enter? = null
        var popExitTransition: Exit? = null
        var sizeTransform: (AnimatedContentTransitionScope<NavBackStackEntry>.() -> SizeTransform?)? = null
    }

    companion object {
        internal const val NAME = "composable"
    }
}

/** Shows [Destination]s as dialogs over the NavHost. */
class DialogNavigator : Navigator<DialogNavigator.Destination>() {
    override val name: String get() = NAME

    override fun createDestination(): Destination = Destination(this) {}

    class Destination(
        navigator: DialogNavigator,
        internal val dialogProperties: DialogProperties = DialogProperties(),
        internal val content: @Composable (NavBackStackEntry) -> Unit,
    ) : NavDestination(NAME) {
        constructor(navigator: DialogNavigator, content: @Composable (NavBackStackEntry) -> Unit) : this(navigator, DialogProperties(), content)
    }

    companion object {
        internal const val NAME = "dialog"
    }
}

/** A nested graph built by `navigation { }` in Compose, with optional transitions for its destinations. */
class ComposeNavGraphNavigator(provider: NavigatorProvider) : NavGraphNavigator(provider) {
    override fun createDestination(): NavGraph = ComposeNavGraph(this)

    class ComposeNavGraph(navGraphNavigator: Navigator<out NavGraph>) : NavGraph(navGraphNavigator) {
        internal var enterTransition: Enter? = null
        internal var exitTransition: Exit? = null
        internal var popEnterTransition: Enter? = null
        internal var popExitTransition: Exit? = null
        internal var sizeTransform: (AnimatedContentTransitionScope<NavBackStackEntry>.() -> SizeTransform?)? = null
    }
}

private fun NavDestination.addAll(arguments: List<NamedNavArgument>, deepLinks: List<NavDeepLink>) {
    arguments.forEach { addArgument(it.name, it.argument) }
    deepLinks.forEach { addDeepLink(it) }
}

/** A composable destination at [route]. */
fun NavGraphBuilder.composable(
    route: String,
    arguments: List<NamedNavArgument> = emptyList(),
    deepLinks: List<NavDeepLink> = emptyList(),
    enterTransition: (AnimatedContentTransitionScope<NavBackStackEntry>.() -> EnterTransition?)? = null,
    exitTransition: (AnimatedContentTransitionScope<NavBackStackEntry>.() -> ExitTransition?)? = null,
    popEnterTransition: (AnimatedContentTransitionScope<NavBackStackEntry>.() -> EnterTransition?)? = enterTransition,
    popExitTransition: (AnimatedContentTransitionScope<NavBackStackEntry>.() -> ExitTransition?)? = exitTransition,
    sizeTransform: (AnimatedContentTransitionScope<NavBackStackEntry>.() -> SizeTransform?)? = null,
    content: @Composable AnimatedContentScope.(NavBackStackEntry) -> Unit,
) {
    addDestination(
        ComposeNavigator.Destination(ComposeNavigator(), content).apply {
            this.route = route
            addAll(arguments, deepLinks)
            this.enterTransition = enterTransition
            this.exitTransition = exitTransition
            this.popEnterTransition = popEnterTransition
            this.popExitTransition = popExitTransition
            this.sizeTransform = sizeTransform
        },
    )
}

@Deprecated("Deprecated in favor of composable builder that supports AnimatedContent", level = DeprecationLevel.HIDDEN)
fun NavGraphBuilder.composable(
    route: String,
    arguments: List<NamedNavArgument> = emptyList(),
    deepLinks: List<NavDeepLink> = emptyList(),
    content: @Composable (NavBackStackEntry) -> Unit,
) = composable(route, arguments, deepLinks) { content(it) }

/** A nested graph with its own start destination and optional transitions for its destinations. */
fun NavGraphBuilder.navigation(
    startDestination: String,
    route: String,
    arguments: List<NamedNavArgument> = emptyList(),
    deepLinks: List<NavDeepLink> = emptyList(),
    enterTransition: (AnimatedContentTransitionScope<NavBackStackEntry>.() -> EnterTransition?)? = null,
    exitTransition: (AnimatedContentTransitionScope<NavBackStackEntry>.() -> ExitTransition?)? = null,
    popEnterTransition: (AnimatedContentTransitionScope<NavBackStackEntry>.() -> EnterTransition?)? = enterTransition,
    popExitTransition: (AnimatedContentTransitionScope<NavBackStackEntry>.() -> ExitTransition?)? = exitTransition,
    sizeTransform: (AnimatedContentTransitionScope<NavBackStackEntry>.() -> SizeTransform?)? = null,
    builder: NavGraphBuilder.() -> Unit,
) {
    val graph = NavGraphBuilder(provider, startDestination, route).apply(builder).let { b ->
        val g = ComposeNavGraphNavigator.ComposeNavGraph(ComposeNavGraphNavigator(provider))
        g.route = route
        // Re-run the builder's destinations into the Compose graph.
        b.build().toList().forEach { g.addDestination(it) }
        g.setStartDestination(startDestination)
        g
    }
    arguments.forEach { graph.addArgument(it.name, it.argument) }
    deepLinks.forEach { graph.addDeepLink(it) }
    graph.enterTransition = enterTransition
    graph.exitTransition = exitTransition
    graph.popEnterTransition = popEnterTransition
    graph.popExitTransition = popExitTransition
    graph.sizeTransform = sizeTransform
    addDestination(graph)
}

/** A dialog destination at [route], shown over the NavHost's content. */
fun NavGraphBuilder.dialog(
    route: String,
    arguments: List<NamedNavArgument> = emptyList(),
    deepLinks: List<NavDeepLink> = emptyList(),
    dialogProperties: DialogProperties = DialogProperties(),
    content: @Composable (NavBackStackEntry) -> Unit,
) {
    addDestination(
        DialogNavigator.Destination(DialogNavigator(), dialogProperties, content).apply {
            this.route = route
            addAll(arguments, deepLinks)
        },
    )
}
