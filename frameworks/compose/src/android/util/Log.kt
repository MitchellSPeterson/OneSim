package android.util

/** Logcat goes to the guest's log sink (stderr on JVM, console on Wasm). */
object Log {
    private fun w(level: String, tag: String?, msg: String, tr: Throwable?): Int {
        onesim.compose.Guest.log("$level/$tag: $msg" + (tr?.let { "\n" + it.stackTraceToString() } ?: ""))
        return 0
    }
    fun v(tag: String?, msg: String, tr: Throwable? = null) = w("V", tag, msg, tr)
    fun d(tag: String?, msg: String, tr: Throwable? = null) = w("D", tag, msg, tr)
    fun i(tag: String?, msg: String, tr: Throwable? = null) = w("I", tag, msg, tr)
    fun w(tag: String?, msg: String, tr: Throwable? = null) = w("W", tag, msg, tr)
    fun e(tag: String?, msg: String, tr: Throwable? = null) = w("E", tag, msg, tr)
}
