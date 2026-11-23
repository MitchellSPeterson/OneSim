package androidx.navigation

/** How a `navigate` call changes the back stack. */
class NavOptions internal constructor(
    private val singleTop: Boolean,
    private val restoreState: Boolean,
    val popUpToId: Int,
    val popUpToRoute: String?,
    private val popUpToInclusive: Boolean,
    private val popUpToSaveState: Boolean,
    val enterAnim: Int = -1,
    val exitAnim: Int = -1,
    val popEnterAnim: Int = -1,
    val popExitAnim: Int = -1,
) {
    fun shouldLaunchSingleTop(): Boolean = singleTop
    fun shouldRestoreState(): Boolean = restoreState
    fun isPopUpToInclusive(): Boolean = popUpToInclusive
    fun shouldPopUpToSaveState(): Boolean = popUpToSaveState

    override fun equals(other: Any?) = other is NavOptions && singleTop == other.singleTop && restoreState == other.restoreState &&
        popUpToId == other.popUpToId && popUpToRoute == other.popUpToRoute && popUpToInclusive == other.popUpToInclusive &&
        popUpToSaveState == other.popUpToSaveState
    override fun hashCode() = listOf(singleTop, restoreState, popUpToId, popUpToRoute, popUpToInclusive, popUpToSaveState).hashCode()

    class Builder {
        private var singleTop = false
        private var restoreState = false
        private var popUpToId = -1
        private var popUpToRoute: String? = null
        private var inclusive = false
        private var saveState = false

        fun setLaunchSingleTop(singleTop: Boolean) = apply { this.singleTop = singleTop }
        fun setRestoreState(restoreState: Boolean) = apply { this.restoreState = restoreState }
        fun setPopUpTo(destinationId: Int, inclusive: Boolean, saveState: Boolean = false) = apply {
            popUpToId = destinationId
            popUpToRoute = null
            this.inclusive = inclusive
            this.saveState = saveState
        }
        fun setPopUpTo(route: String?, inclusive: Boolean, saveState: Boolean = false) = apply {
            popUpToRoute = route
            popUpToId = -1
            this.inclusive = inclusive
            this.saveState = saveState
        }
        fun build() = NavOptions(singleTop, restoreState, popUpToId, popUpToRoute, inclusive, saveState)
    }
}

@DslMarker
annotation class NavOptionsDsl

@NavOptionsDsl
class PopUpToBuilder {
    var inclusive: Boolean = false
    var saveState: Boolean = false
}

@NavOptionsDsl
class NavOptionsBuilder {
    private val builder = NavOptions.Builder()
    var launchSingleTop: Boolean = false
    var restoreState: Boolean = false
    var popUpToId: Int = -1
        private set
    var popUpToRoute: String? = null
        private set
    private var inclusive = false
    private var saveState = false

    fun popUpTo(id: Int, popUpToBuilder: PopUpToBuilder.() -> Unit = {}) {
        popUpToId = id
        popUpToRoute = null
        val b = PopUpToBuilder().apply(popUpToBuilder)
        inclusive = b.inclusive
        saveState = b.saveState
    }

    fun popUpTo(route: String, popUpToBuilder: PopUpToBuilder.() -> Unit = {}) {
        require(route.isNotBlank()) { "Cannot pop up to an empty route" }
        popUpToRoute = route
        popUpToId = -1
        val b = PopUpToBuilder().apply(popUpToBuilder)
        inclusive = b.inclusive
        saveState = b.saveState
    }

    internal fun build(): NavOptions {
        builder.setLaunchSingleTop(launchSingleTop).setRestoreState(restoreState)
        if (popUpToRoute != null) builder.setPopUpTo(popUpToRoute, inclusive, saveState)
        else if (popUpToId != -1) builder.setPopUpTo(popUpToId, inclusive, saveState)
        return builder.build()
    }
}

fun navOptions(optionsBuilder: NavOptionsBuilder.() -> Unit): NavOptions = NavOptionsBuilder().apply(optionsBuilder).build()
