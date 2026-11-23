package androidx.activity.compose

import androidx.activity.BackEventCompat
import androidx.activity.OnBackPressedCallback
import androidx.activity.OnBackPressedDispatcherOwner
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.ProvidableCompositionLocal
import androidx.compose.runtime.ProvidedValue
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.consumeAsFlow
import kotlinx.coroutines.flow.onCompletion
import kotlinx.coroutines.launch
import androidx.lifecycle.compose.LocalLifecycleOwner
import onesim.compose.Guest

/** The [OnBackPressedDispatcherOwner] of the composition: the activity unless something provides another. */
object LocalOnBackPressedDispatcherOwner {
    private val local: ProvidableCompositionLocal<OnBackPressedDispatcherOwner?> = compositionLocalOf { null }

    val current: OnBackPressedDispatcherOwner?
        @Composable get() = local.current ?: Guest.activity

    infix fun provides(dispatcherOwner: OnBackPressedDispatcherOwner): ProvidedValue<OnBackPressedDispatcherOwner?> =
        local.provides(dispatcherOwner)
}

/**
 * Calls [onBack] when system back is committed (the back key, or a back swipe released past its trigger) while
 * [enabled]. The most recently composed enabled handler wins; a back gesture shows no in-app progress.
 */
@Composable
fun BackHandler(enabled: Boolean = true, onBack: () -> Unit) {
    val currentOnBack by rememberUpdatedState(onBack)
    val callback = remember {
        object : OnBackPressedCallback(enabled) {
            override fun handleOnBackPressed() = currentOnBack()
        }
    }
    SideEffect { callback.isEnabled = enabled }
    val owner = checkNotNull(LocalOnBackPressedDispatcherOwner.current) {
        "No OnBackPressedDispatcherOwner was provided via LocalOnBackPressedDispatcherOwner"
    }
    val dispatcher = owner.onBackPressedDispatcher
    // Registered while the composition's lifecycle owner (a NavHost entry, else the activity) is started: a
    // destination leaving the screen stops handling back.
    val lifecycleOwner = LocalLifecycleOwner.current
    DisposableEffect(lifecycleOwner, dispatcher) {
        dispatcher.addCallback(lifecycleOwner, callback)
        onDispose { callback.remove() }
    }
}

/**
 * Handles predictive back while [enabled]: a back gesture starts [onBack] with a flow of its progress events; the flow
 * completes when the back commits (and [onBack] should finish its work), or the coroutine is cancelled when the
 * gesture is cancelled. The back key runs [onBack] with an empty flow. [onBack] must collect the flow.
 */
@Composable
fun PredictiveBackHandler(enabled: Boolean = true, onBack: suspend (progress: Flow<BackEventCompat>) -> Unit) {
    val currentOnBack by rememberUpdatedState(onBack)
    val scope = rememberCoroutineScope()
    val callback = remember {
        object : OnBackPressedCallback(enabled) {
            var instance: OnBackInstance? = null

            override fun handleOnBackStarted(backEvent: BackEventCompat) {
                instance?.cancel()
                if (isEnabled) instance = OnBackInstance(scope, true, currentOnBack)
            }

            override fun handleOnBackProgressed(backEvent: BackEventCompat) {
                instance?.send(backEvent)
            }

            override fun handleOnBackPressed() {
                if (instance?.isPredictiveBack == false) {
                    instance?.cancel()
                    instance = null
                }
                if (instance == null) instance = OnBackInstance(scope, false, currentOnBack)
                instance?.close()
                instance?.isPredictiveBack = false
                instance = null
            }

            override fun handleOnBackCancelled() {
                instance?.cancel()
                instance?.isPredictiveBack = false
                instance = null
            }
        }
    }
    SideEffect { callback.isEnabled = enabled }
    val owner = checkNotNull(LocalOnBackPressedDispatcherOwner.current) {
        "No OnBackPressedDispatcherOwner was provided via LocalOnBackPressedDispatcherOwner"
    }
    val dispatcher = owner.onBackPressedDispatcher
    // Registered while the composition's lifecycle owner (a NavHost entry, else the activity) is started: a
    // destination leaving the screen stops handling back.
    val lifecycleOwner = LocalLifecycleOwner.current
    DisposableEffect(lifecycleOwner, dispatcher) {
        dispatcher.addCallback(lifecycleOwner, callback)
        onDispose { callback.remove() }
    }
}

/** One run of a [PredictiveBackHandler]'s `onBack` and the channel feeding its progress flow. */
private class OnBackInstance(
    scope: CoroutineScope,
    var isPredictiveBack: Boolean,
    onBack: suspend (progress: Flow<BackEventCompat>) -> Unit,
) {
    val channel = Channel<BackEventCompat>(capacity = Channel.BUFFERED, onBufferOverflow = BufferOverflow.SUSPEND)
    val job: Job = scope.launch {
        var completed = false
        onBack(channel.consumeAsFlow().onCompletion { completed = true })
        check(completed) { "You must collect the progress flow" }
    }

    fun send(backEvent: BackEventCompat) = channel.trySend(backEvent)

    fun close() = channel.close()

    fun cancel() {
        channel.cancel(CancellationException("onBack cancelled"))
        job.cancel()
    }
}
