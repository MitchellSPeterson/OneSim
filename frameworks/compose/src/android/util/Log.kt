package android.util

/** Logcat goes to the guest's stderr (the host's session log). */
object Log {
    private fun w(level: String, tag: String?, msg: String, tr: Throwable?): Int {
        System.err.println("$level/$tag: $msg" + (tr?.let { "\n" + it.stackTraceToString() } ?: ""))
        return 0
    }
    @JvmStatic fun v(tag: String?, msg: String, tr: Throwable? = null) = w("V", tag, msg, tr)
    @JvmStatic fun d(tag: String?, msg: String, tr: Throwable? = null) = w("D", tag, msg, tr)
    @JvmStatic fun i(tag: String?, msg: String, tr: Throwable? = null) = w("I", tag, msg, tr)
    @JvmStatic fun w(tag: String?, msg: String, tr: Throwable? = null) = w("W", tag, msg, tr)
    @JvmStatic fun e(tag: String?, msg: String, tr: Throwable? = null) = w("E", tag, msg, tr)
}
