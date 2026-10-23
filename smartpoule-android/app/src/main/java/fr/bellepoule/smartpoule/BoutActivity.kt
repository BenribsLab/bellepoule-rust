package fr.bellepoule.smartpoule

import android.content.Context
import android.app.Dialog
import android.graphics.Color
import android.graphics.Typeface
import android.media.ToneGenerator
import android.media.AudioManager
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.appcompat.widget.Toolbar
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

/** Chrono de piste, touches et journal des cartons, rattachés au match. */
class BoutActivity : AppCompatActivity() {
    private lateinit var api: Api
    private lateinit var token: String
    private lateinit var cacheKey: String
    private lateinit var clock: BoutClock
    private var bout = 0
    private var max = 5
    private var winner = -1
    private var revision = 0L
    private val scores = intArrayOf(0, 0)
    private val names = arrayOf("", "")
    private val counters = arrayOfNulls<TextView>(2)
    private val cardCounts = arrayOfNulls<TextView>(2)
    private var cards = JSONArray()
    // Indices stables de la feuille : premier nom = 0, vert par défaut à droite.
    private var displayOrder = intArrayOf(1, 0)
    private lateinit var board: LinearLayout
    private lateinit var cardRow: LinearLayout
    private val scorePanels = arrayOfNulls<LinearLayout>(2)
    private val cardPanels = arrayOfNulls<LinearLayout>(2)
    private lateinit var timer: TextView
    private lateinit var round: TextView
    private lateinit var toggle: Button
    private lateinit var send: Button
    private lateinit var status: TextView
    private lateinit var syncStatus: TextView
    private lateinit var history: TextView
    private lateinit var next: Button
    private val adjustmentsButtons = mutableListOf<Button>()
    private var alarm: ToneGenerator? = null
    private var cardDisplay: Dialog? = null
    private var sending = false
    private var syncing = false
    private var pending: JSONObject? = null
    // Écran LivePoule de la piste : dernier état envoyé.
    private var piste = 0
    private var competition = ""
    private var color = ""
    private var liveInFlight = false
    private var lastLive = ""
    private var lastLiveAt = 0L
    private val handler = Handler(Looper.getMainLooper())
    private val ticker = object : Runnable {
        override fun run() {
            if (isFinishing || isDestroyed) return
            if (clock.running && clock.remaining(now()) == 0L) {
                clock.pause(now())
                alarm?.release()
                alarm = ToneGenerator(AudioManager.STREAM_NOTIFICATION, 80)
                alarm?.startTone(ToneGenerator.TONE_PROP_BEEP2, 600)
                handler.postDelayed({ alarm?.release(); alarm = null }, 700)
                persist()
            }
            refreshClock()
            pushLive()
            handler.postDelayed(this, 100)
        }
    }
    private fun now() = SystemClock.elapsedRealtime()
    private fun cache() = getSharedPreferences("smartpoule-bouts", Context.MODE_PRIVATE)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val pairing = Pairing.load(this) ?: return finish()
        api = Api(pairing)
        token = intent.getStringExtra("token") ?: return finish()
        bout = intent.getIntExtra("bout", 0)
        piste = intent.getIntExtra("piste", 0)
        competition = intent.getStringExtra("competition") ?: ""
        color = intent.getStringExtra("color") ?: ""
        max = intent.getIntExtra("max", 5).coerceAtLeast(1)
        cacheKey = "${pairing.server}|${pairing.club}|${pairing.referee}|$token|$bout"
        val timing = JSONObject(intent.getStringExtra("timing") ?: "{}")
        clock = BoutClock(timing.optInt("period_seconds", 180).coerceIn(1, 3600), timing.optInt("periods", 1).coerceIn(1, 20))
        for (k in 0..1) {
            val f = JSONObject(intent.getStringExtra(if (k == 0) "a" else "b") ?: "{}")
            names[k] = f.optString("name")
            val image = f.optString("score").trim().uppercase()
            scores[k] = if (image == "V") max else image.trimStart('V', 'W').toIntOrNull() ?: 0
            if (image.startsWith("V") || image.startsWith("W")) winner = k
        }
        restore()
        title = intent.getStringExtra("title") ?: "Match"
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        val content = Ui.column(this, 10)
        content.addView(Ui.text(this, "$max touches · ${clock.periods} manche(s) de ${clock.periodSeconds / 60} min ${clock.periodSeconds % 60} s", 13f, color = Ui.MUTED))
        timer = Ui.text(this, "03:00", 66f, true, Ui.INK).apply {
            gravity = Gravity.CENTER
            typeface = Typeface.create(Typeface.MONOSPACE, Typeface.BOLD)
            background = Ui.rounded(0xFFE9EDF2.toInt(), 0xFFA7B2C3.toInt(), 8f, this@BoutActivity)
            contentDescription = "Temps restant"
        }
        content.addView(timer, LinearLayout.LayoutParams(-1, Ui.dp(this, 100)))
        val adjustments = Ui.row(this)
        for ((text, delta) in listOf("− 1 s" to -1000L, "+ 1 s" to 1000L, "− 1 min" to -60000L, "+ 1 min" to 60000L)) {
            adjustments.addView(Ui.button(this, text, false) {
                if (!clock.running && !sending) { clock.setRemaining(clock.remaining(now()) + delta); persist(); refresh() }
            }.apply { textSize = 12f; adjustmentsButtons.add(this) }, LinearLayout.LayoutParams(0, Ui.dp(this, 42), 1f))
        }
        content.addView(adjustments)
        content.addView(Ui.button(this, "Inverser les côtés rouge / vert", false) {
            clock.pause(now())
            displayOrder.reverse()
            arrangeSides()
            persist()
            refresh()
        })
        board = Ui.row(this).apply { setBackgroundColor(Color.BLACK) }
        for (k in 0..1) {
            val side = Ui.column(this, 8)
            val color = if (k == displayOrder[0]) 0xFFFF5252.toInt() else 0xFF00E676.toInt()
            side.addView(Ui.text(this, names[k], 16f, true, Color.WHITE).apply { gravity = Gravity.CENTER; maxLines = 2; setOnClickListener { winner = k; refresh() } })
            counters[k] = Ui.text(this, "0", 64f, true, color).apply {
                gravity = Gravity.CENTER
                setOnClickListener { winner = k; refresh() }
            }
            side.addView(counters[k])
            val controls = Ui.row(this)
            for ((label, delta) in listOf("−" to -1, "+" to 1)) {
                controls.addView(Ui.button(this, label, false) { change(k, delta) }.apply { textSize = 25f }, LinearLayout.LayoutParams(0, Ui.dp(this, 48), 1f))
            }
            side.addView(controls)
            scorePanels[k] = side
        }
        content.addView(board)
        toggle = Ui.button(this, "Démarrer") {
            if (clock.running) { clock.pause(now()); persist() }
            else { clock.start(now()); saveLocal() }
            refresh()
        }.apply { textSize = 24f }
        content.addView(toggle, LinearLayout.LayoutParams(-1, Ui.dp(this, 64)))
        round = Ui.text(this, "", 15f, true).apply { gravity = Gravity.CENTER }
        content.addView(round)
        val rounds = Ui.row(this)
        rounds.addView(Ui.button(this, "Réinitialiser le chrono", false) {
            AlertDialog.Builder(this).setTitle("Réinitialiser cette manche ?")
                .setMessage("Le score et les cartons sont conservés.")
                .setPositiveButton("Réinitialiser") { _, _ -> clock.reset(now()); persist(); refresh() }
                .setNegativeButton("Annuler", null).show()
        }, LinearLayout.LayoutParams(0, -2, 1f))
        next = Ui.button(this, "Manche suivante", false) {
            AlertDialog.Builder(this).setTitle("Passer à la manche suivante ?")
                .setPositiveButton("Continuer") { _, _ -> clock.next(now()); persist(); refresh() }
                .setNegativeButton("Annuler", null).show()
        }
        rounds.addView(next, LinearLayout.LayoutParams(0, -2, 1f))
        content.addView(rounds)
        cardRow = Ui.row(this)
        for (k in 0..1) {
            val panel = Ui.column(this, 4)
            panel.addView(Ui.text(this, names[k], 13f, true))
            val actions = Ui.row(this)
            for ((color, label, fill) in listOf(Triple("yellow", "Jaune", 0xFFFFDC58.toInt()), Triple("red", "Rouge", 0xFFE94B55.toInt()), Triple("black", "Noir", 0xFF263238.toInt()))) {
                val button = Ui.button(this, label) { issueCard(k, color, label) }.apply {
                    textSize = 11f
                    setTextColor(if (color == "yellow") Color.BLACK else Color.WHITE)
                    background = Ui.rounded(fill, fill, 4f, this@BoutActivity)
                }
                actions.addView(button, LinearLayout.LayoutParams(0, Ui.dp(this, 46), 1f))
            }
            panel.addView(actions)
            cardCounts[k] = Ui.text(this, "", 12f, color = Ui.MUTED)
            panel.addView(cardCounts[k])
            cardPanels[k] = panel
        }
        arrangeSides()
        content.addView(cardRow)
        content.addView(Ui.button(this, "Annuler le dernier carton", false) { undoCard() })
        history = Ui.text(this, "", 12f, color = Ui.MUTED)
        content.addView(history)
        status = Ui.text(this, "", 14f).apply { gravity = Gravity.CENTER }
        content.addView(status)
        syncStatus = Ui.text(this, "", 12f, color = Ui.MUTED)
        content.addView(syncStatus)
        content.addView(Ui.button(this, "Synchroniser chrono et cartons", false) { persist() })
        send = Ui.button(this, "Valider et envoyer le résultat") { confirm() }
        content.addView(send)
        // La barre et le match partagent un agencement vertical : aucun recouvrement,
        // y compris avec l'affichage bord à bord imposé sur les Android récents.
        WindowCompat.setDecorFitsSystemWindows(window, false)
        val screen = Ui.column(this, 0).apply { setBackgroundColor(Color.WHITE) }
        ViewCompat.setOnApplyWindowInsetsListener(screen) { view, insets ->
            val safe = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            view.setPadding(safe.left, safe.top, safe.right, safe.bottom)
            insets
        }
        val toolbar = Toolbar(this).apply {
            setBackgroundColor(Ui.BLUE)
            setTitleTextColor(Color.WHITE)
            minimumHeight = Ui.dp(this@BoutActivity, 56)
        }
        screen.addView(toolbar, LinearLayout.LayoutParams(-1, -2))
        screen.addView(ScrollView(this).apply { addView(content) }, LinearLayout.LayoutParams(-1, 0, 1f))
        setContentView(screen)
        setSupportActionBar(toolbar)
        supportActionBar?.setDisplayHomeAsUpEnabled(true)
        toolbar.navigationIcon?.setTint(Color.WHITE)
        ViewCompat.requestApplyInsets(screen)
        refresh()
        handler.post(ticker)
        if (cache().getBoolean("$cacheKey.pending", false)) { pending = snapshot(); flush() }
    }

    private fun arrangeSides() {
        board.removeAllViews()
        cardRow.removeAllViews()
        for (position in 0..1) {
            val player = displayOrder[position]
            counters[player]?.setTextColor(if (position == 0) 0xFFFF5252.toInt() else 0xFF00E676.toInt())
            scorePanels[player]?.let { board.addView(it, LinearLayout.LayoutParams(0, -2, 1f)) }
            cardPanels[player]?.let { cardRow.addView(it, LinearLayout.LayoutParams(0, -2, 1f)) }
        }
    }

    private fun restore() {
        val remote = intent.getStringExtra("officiating")?.let { runCatching { JSONObject(it) }.getOrNull() }
        val local = cache().getString(cacheKey, null)?.let { runCatching { JSONObject(it) }.getOrNull() }
        val chosen = if (local != null && local.optLong("revision") >= (remote?.optLong("revision") ?: 0L)) local else remote
        chosen ?: return
        revision = chosen.optLong("revision")
        displayOrder = if (chosen.optInt("green_side", 0) == 0) intArrayOf(1, 0) else intArrayOf(0, 1)
        clock.period = chosen.optInt("period", 1).coerceIn(1, clock.periods)
        var remaining = chosen.optLong("remaining_ms", clock.periodSeconds * 1000L)
        if (chosen.optBoolean("running")) {
            val saved = chosen.optLong("saved_elapsed", now())
            if (now() >= saved) remaining -= now() - saved
        }
        clock.setRemaining(remaining)
        cards = chosen.optJSONArray("cards") ?: JSONArray()
        if (!intent.getBooleanExtra("over", false)) chosen.optJSONArray("scores")?.let { for (k in 0..1) scores[k] = it.optInt(k).coerceIn(0, max) }
    }

    private fun snapshot() = JSONObject().put("revision", revision).put("period", clock.period)
        .put("remaining_ms", clock.remaining(now())).put("scores", JSONArray().put(scores[0]).put(scores[1]))
        .put("green_side", displayOrder[1]).put("cards", JSONArray(cards.toString()))

    private fun saveLocal() {
        cache().edit().putString(cacheKey, snapshot().put("running", clock.running).put("saved_elapsed", now()).toString()).apply()
    }

    private fun persist() {
        revision++
        saveLocal()
        cache().edit().putBoolean("$cacheKey.pending", true).apply()
        pending = snapshot()
        flush()
    }

    private fun flush() {
        if (syncing || sending || !::syncStatus.isInitialized) return
        val state = pending ?: return
        pending = null
        syncing = true
        api.officiating(token, bout, state) { result ->
            syncing = false
            result.onSuccess { answer ->
                if (answer.optString("status") == "pending") {
                    syncStatus.text = "Transmis au serveur ; en attente de ManageComp."
                    followState(answer.optLong("id"), state.optLong("revision"), 0)
                } else {
                    acknowledged(state.optLong("revision"))
                }
            }.onFailure {
                pending = pending ?: state
                syncStatus.text = "Conservé sur ce téléphone — ${it.message}"
            }
            if (result.isSuccess) flush()
        }
    }

    private fun acknowledged(sentRevision: Long) {
        if (sentRevision == revision) {
            cache().edit().putBoolean("$cacheKey.pending", false).apply()
            syncStatus.text = "Chrono et cartons enregistrés."
        }
    }

    private fun followState(id: Long, sentRevision: Long, attempt: Int) {
        if (isFinishing || isDestroyed || attempt > 40) return
        handler.postDelayed({ api.result(id) { result ->
            result.onSuccess { answer -> when (answer.optString("status")) {
                "ok" -> acknowledged(sentRevision)
                "error" -> { if (sentRevision == revision) { syncStatus.text = answer.optString("error"); cache().edit().putBoolean("$cacheKey.pending", true).apply() } }
                else -> followState(id, sentRevision, attempt + 1)
            } }.onFailure { followState(id, sentRevision, attempt + 1) }
        } }, 1500)
    }

    private fun change(side: Int, delta: Int) {
        if (sending) return
        clock.pause(now())
        scores[side] = (scores[side] + delta).coerceIn(0, max)
        persist()
        refresh()
    }

    private fun issueCard(side: Int, color: String, label: String) {
        if (sending || cardDisplay != null) return
        clock.pause(now()); saveLocal(); refresh()
        val fill = when (color) {
            "yellow" -> Color.YELLOW
            "red" -> Color.RED
            else -> Color.BLACK
        }
        val display = Dialog(this, android.R.style.Theme_Black_NoTitleBar_Fullscreen)
        val surface = View(this).apply {
            setBackgroundColor(fill)
            contentDescription = "Carton $label pour ${names[side]}. Touchez pour passer à la validation."
            isFocusable = true
            setOnClickListener {
                display.dismiss()
                confirmCard(side, color, label)
            }
        }
        display.setContentView(surface)
        display.setCanceledOnTouchOutside(false)
        display.setOnDismissListener { cardDisplay = null }
        cardDisplay = display
        display.show()
        display.window?.let { cardWindow ->
            cardWindow.setLayout(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
            cardWindow.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            if (android.os.Build.VERSION.SDK_INT >= 28) {
                cardWindow.attributes = cardWindow.attributes.apply {
                    layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
                }
            }
            WindowCompat.setDecorFitsSystemWindows(cardWindow, false)
            WindowInsetsControllerCompat(cardWindow, surface).apply {
                systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                hide(WindowInsetsCompat.Type.systemBars())
            }
        }
    }

    private fun confirmCard(side: Int, color: String, label: String) {
        if (isFinishing || isDestroyed) return
        AlertDialog.Builder(this).setTitle("Carton $label — ${names[side]}")
            .setMessage("Manche ${clock.period} · ${clock.display(now())} restant")
            .setPositiveButton("Enregistrer") { _, _ ->
                if (cards.length() >= 256) { syncStatus.text = "Journal complet (256 événements)."; return@setPositiveButton }
                cards.put(JSONObject().put("id", UUID.randomUUID().toString()).put("side", side).put("color", color)
                    .put("period", clock.period).put("remaining_ms", clock.remaining(now())).put("issued_at_ms", System.currentTimeMillis()).put("cancelled", false))
                persist(); refresh()
            }.setNegativeButton("Annuler", null).show()
    }

    private fun undoCard() {
        if (sending) return
        val index = (cards.length() - 1 downTo 0).firstOrNull { !cards.getJSONObject(it).optBoolean("cancelled") } ?: return
        AlertDialog.Builder(this).setTitle("Annuler ce carton ?").setMessage(cardText(cards.getJSONObject(index)))
            .setPositiveButton("Annuler le carton") { _, _ -> clock.pause(now()); cards.getJSONObject(index).put("cancelled", true); persist(); refresh() }
            .setNegativeButton("Conserver", null).show()
    }

    private fun cardText(card: JSONObject): String {
        val seconds = (card.optLong("remaining_ms") + 999) / 1000
        val color = when (card.optString("color")) { "yellow" -> "jaune"; "red" -> "rouge"; else -> "noir" }
        return "${names[card.optInt("side").coerceIn(0, 1)]} : $color · M${card.optInt("period")} %02d:%02d".format(seconds / 60, seconds % 60) + if (card.optBoolean("cancelled")) " (annulé)" else ""
    }

    private fun currentWinner(): Int = when { scores[0] > scores[1] -> 0; scores[1] > scores[0] -> 1; else -> winner }

    private fun refreshClock() {
        timer.text = clock.display(now())
        timer.setTextColor(if (clock.remaining(now()) == 0L) Ui.RED else Ui.INK)
        toggle.text = if (clock.running) "Pause" else "Démarrer"
        val fill = if (clock.running) 0xFFFFA000.toInt() else 0xFF00BA32.toInt()
        toggle.background = Ui.rounded(fill, fill, 6f, this)
        toggle.isEnabled = !sending && clock.remaining(now()) > 0L
        round.text = "Manche ${clock.period} / ${clock.periods}"
        next.isEnabled = !sending && !clock.running && clock.period < clock.periods
        adjustmentsButtons.forEach { it.isEnabled = !sending && !clock.running }
    }

    private fun refresh() {
        refreshClock()
        for (side in 0..1) {
            counters[side]?.text = scores[side].toString()
            cardCounts[side]?.text = listOf("yellow" to "J", "red" to "R", "black" to "N").joinToString(" · ") { (color, label) ->
                val count = (0 until cards.length()).count {
                    val card = cards.getJSONObject(it)
                    card.optInt("side") == side && card.optString("color") == color && !card.optBoolean("cancelled")
                }
                "$label : $count"
            }
        }
        history.text = (0 until cards.length()).joinToString("\n") { cardText(cards.getJSONObject(it)) }
        status.text = currentWinner().let { if (it < 0) "Égalité : touchez le nom du vainqueur avant validation." else "Vainqueur : ${names[it]}" }
    }

    private fun confirm() {
        clock.pause(now()); refresh()
        val w = currentWinner()
        if (w < 0 || (scores[0] == max && scores[1] == max)) { status.text = "Choisir un seul vainqueur et vérifier les touches."; return }
        AlertDialog.Builder(this).setTitle("Valider ce résultat ?")
            .setMessage("${names[0]} : ${scores[0]}\n${names[1]} : ${scores[1]}\nManche ${clock.period} · ${clock.display(now())}\nVainqueur : ${names[w]}\nCartons : ${cards.length()} événement(s)")
            .setPositiveButton("Envoyer") { _, _ -> submit(w) }.setNegativeButton("Annuler", null).show()
    }

    private fun submit(w: Int) {
        clock.pause(now()); revision++; saveLocal()
        sending = true; send.isEnabled = false; status.text = "Envoi…"
        val values = (0..1).map { if (it == w) "W${scores[it]}" else "${scores[it]}" }
        api.score(token, bout, values[0], values[1], snapshot()) { result ->
            result.onSuccess { answer -> when (answer.optString("status")) {
                "pending" -> pollResult(answer.optLong("id"), 0)
                "error" -> failed(answer.optString("error"))
                else -> done()
            } }.onFailure { failed(it.message ?: "Erreur") }
        }
    }

    private fun pollResult(id: Long, attempt: Int) {
        if (isFinishing || isDestroyed) return
        if (attempt > 40) { failed("Résultat transmis, validation ManageComp encore attendue."); return }
        status.text = "En attente de ManageComp…"
        handler.postDelayed({ api.result(id) { result ->
            result.onSuccess { answer -> when (answer.optString("status")) {
                "ok" -> done(); "error" -> failed(answer.optString("error")); else -> pollResult(id, attempt + 1)
            } }.onFailure { pollResult(id, attempt + 1) }
        } }, 1500)
    }

    /** État affiché sur l'écran LivePoule : rouge à gauche, vert à droite, comme sur le téléphone. */
    private fun liveState(over: Boolean): JSONObject {
        val red = displayOrder[0]
        val green = displayOrder[1]
        return JSONObject()
            .put("piste", piste)
            .put("competition", competition)
            .put("color", color)
            .put("bout", intent.getStringExtra("title") ?: "")
            .put("red", JSONObject().put("name", names[red]).put("score", scores[red].toString()))
            .put("green", JSONObject().put("name", names[green]).put("score", scores[green].toString()))
            .put("remaining_ms", clock.remaining(now()))
            .put("running", clock.running)
            .put("period", clock.period)
            .put("periods", clock.periods)
            .put("over", over)
    }

    /** Envoie l'état en direct à chaque changement, et toutes les 3 s quand le chrono tourne. */
    private fun pushLive() {
        if (piste <= 0 || liveInFlight) return
        val key = "${displayOrder[0]}|${scores[0]}|${scores[1]}|${clock.running}|${clock.period}|" + (if (clock.running) "" else clock.remaining(now()).toString())
        val elapsed = now() - lastLiveAt
        if (key == lastLive && elapsed < (if (clock.running) 3000 else 20000)) return
        lastLive = key
        lastLiveAt = now()
        liveInFlight = true
        api.live(liveState(false)) { liveInFlight = false }
    }

    private fun done() {
        if (piste > 0) api.live(liveState(true)) { }
        cache().edit().putBoolean("$cacheKey.pending", false).apply()
        status.text = "Résultat et cartons enregistrés ✔"
        handler.postDelayed({ finish() }, 800)
    }
    private fun failed(message: String) { sending = false; send.isEnabled = true; status.text = message; flush() }
    override fun onSupportNavigateUp(): Boolean { finish(); return true }
    override fun onSaveInstanceState(outState: Bundle) { saveLocal(); super.onSaveInstanceState(outState) }
    override fun onStop() {
        if (::clock.isInitialized) {
            val running = clock.running
            clock.pause(now())
            if (running && ::syncStatus.isInitialized && !sending) persist() else saveLocal()
        }
        super.onStop()
    }
    override fun onDestroy() { cardDisplay?.dismiss(); cardDisplay = null; handler.removeCallbacksAndMessages(null); alarm?.release(); alarm = null; super.onDestroy() }
}
