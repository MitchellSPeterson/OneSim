package androidx.compose.foundation

import androidx.compose.runtime.Stable
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** Priority of a mutation (a scroll or drag): a new one cancels a running one of equal or lower priority. */
enum class MutatePriority { Default, UserInput, PreventUserInput }

/** Thrown into a mutation cancelled by one of higher or equal priority. */
class MutationInterruptedException : CancellationException("Mutation interrupted")

/** Runs one mutation at a time; starting one cancels the current one unless it has a higher priority. */
@Stable
class MutatorMutex {
    private class Mutator(val priority: MutatePriority, val job: Job) {
        fun canInterrupt(other: Mutator) = priority >= other.priority
        fun cancel() = job.cancel(MutationInterruptedException())
    }

    private var current: Mutator? = null
    private val mutex = Mutex()

    private fun tryMutateOrCancel(mutator: Mutator) {
        val old = current
        if (old == null || mutator.canInterrupt(old)) {
            current = mutator
            old?.cancel()
        } else {
            throw CancellationException("Current mutation had a higher priority")
        }
    }

    suspend fun <R> mutate(priority: MutatePriority = MutatePriority.Default, block: suspend () -> R): R = coroutineScope {
        val mutator = Mutator(priority, coroutineContext[Job]!!)
        tryMutateOrCancel(mutator)
        mutex.withLock {
            try {
                block()
            } finally {
                if (current === mutator) current = null
            }
        }
    }

    suspend fun <T, R> mutateWith(receiver: T, priority: MutatePriority = MutatePriority.Default, block: suspend T.() -> R): R =
        coroutineScope {
            val mutator = Mutator(priority, coroutineContext[Job]!!)
            tryMutateOrCancel(mutator)
            mutex.withLock {
                try {
                    receiver.block()
                } finally {
                    if (current === mutator) current = null
                }
            }
        }

    /** Run [block] if nothing is mutating; returns false otherwise. */
    fun tryMutate(block: () -> Unit): Boolean {
        val didLock = mutex.tryLock()
        if (didLock) {
            try {
                block()
            } finally {
                mutex.unlock()
            }
        }
        return didLock
    }
}
