package fr.bellepoule.smartpoule

/** Décompte monotone, indépendant de la fréquence de rafraîchissement. */
class BoutClock(val periodSeconds: Int, val periods: Int) {
    var period = 1
    var running = false
        private set
    private var storedMs = periodSeconds * 1000L
    private var deadline = 0L
    fun remaining(now: Long): Long = if (running) (deadline - now).coerceAtLeast(0) else storedMs
    fun start(now: Long) { if (!running && storedMs > 0) { deadline = now + storedMs; running = true } }
    fun pause(now: Long) { storedMs = remaining(now); running = false }
    fun setRemaining(ms: Long) { check(!running); storedMs = ms.coerceIn(0, periodSeconds * 1000L) }
    fun reset(now: Long) { pause(now); storedMs = periodSeconds * 1000L }
    fun next(now: Long) { if (period < periods) { reset(now); period++ } }
    fun display(now: Long): String {
        val seconds = (remaining(now) + 999) / 1000
        return "%02d:%02d".format(seconds / 60, seconds % 60)
    }
}
