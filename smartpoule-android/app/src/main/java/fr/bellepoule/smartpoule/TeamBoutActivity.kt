package fr.bellepoule.smartpoule

import android.content.Context
import android.media.AudioManager
import android.media.ToneGenerator
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.Gravity
import android.view.ViewGroup
import android.view.WindowManager
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Spinner
import android.widget.TextView
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import org.json.JSONArray
import org.json.JSONObject

/**
 * Rencontre par équipes en relais : l'arbitre désigne l'équipe du haut
 * (numéros 1-2-3) et celle du bas (4-5-6), note qui tire à chaque numéro,
 * puis saisit les relais dans l'ordre. Les scores sont cumulés : le relais n
 * se termine quand une équipe atteint n × (points par relais), ou au temps.
 */
class TeamBoutActivity : AppCompatActivity() {
    private lateinit var api: Api
    private lateinit var token: String
    private var bout = 0
    private var piste = 0
    private var competition = ""
    private var color = ""
    private lateinit var format: RelayFormat
    private val teams = arrayOf("", "")
    private val members = arrayOf(listOf<String>(), listOf())
    // Journal : équipe du haut (indice dans la feuille), composition, scores cumulés.
    private var top = -1
    private var lineup = mutableListOf<String>()
    private val relays = mutableListOf<IntArray>()
    private var winner = -1
    private var revision = 0L
    // Relais en cours : scores cumulés [haut, bas].
    private val current = intArrayOf(0, 0)
    private lateinit var clock: BoutClock
    private lateinit var content: LinearLayout
    private val handler = Handler(Looper.getMainLooper())
    private var timerView: TextView? = null
    private var toggle: Button? = null
    private var sending = false
    private var liveKey = ""
    private var liveAt = 0L
    private var liveInFlight = false
    private val ticker = object : Runnable {
        override fun run() {
            if (isFinishing || isDestroyed) return
            if (clock.running && clock.remaining(now()) == 0L) {
                clock.pause(now())
                ToneGenerator(AudioManager.STREAM_NOTIFICATION, 80).startTone(ToneGenerator.TONE_PROP_BEEP2, 600)
                showRelay()
            }
            timerView?.text = clock.display(now())
            pushLive()
            handler.postDelayed(this, 200)
        }
    }

    private fun now() = SystemClock.elapsedRealtime()
    private fun cache() = getSharedPreferences("smartpoule-teams", Context.MODE_PRIVATE)
    private val cacheKey get() = "$token|$bout"

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        val pairing = Pairing.load(this) ?: return finish()
        api = Api(pairing)
        token = intent.getStringExtra("token") ?: return finish()
        bout = intent.getIntExtra("bout", 0)
        piste = intent.getIntExtra("piste", 0)
        competition = intent.getStringExtra("competition") ?: ""
        color = intent.getStringExtra("color") ?: ""
        title = intent.getStringExtra("title") ?: "Rencontre par équipes"
        supportActionBar?.setDisplayHomeAsUpEnabled(true)
        format = RelayFormat(JSONObject(intent.getStringExtra("relay") ?: "{}"))
        val timing = JSONObject(intent.getStringExtra("timing") ?: "{}")
        clock = BoutClock(timing.optInt("period_seconds", 180).coerceIn(1, 3600), 1)
        for (k in 0..1) {
            val team = JSONObject(intent.getStringExtra(if (k == 0) "a" else "b") ?: "{}")
            teams[k] = team.optString("name")
            val list = team.optJSONArray("members") ?: JSONArray()
            members[k] = (0 until list.length()).map { list.optString(it) }
        }
        // Reprise : journal local, sinon celui du serveur.
        val saved = cache().getString(cacheKey, null) ?: intent.getStringExtra("relay_log")
        saved?.let { restore(JSONObject(it)) }
        content = Ui.column(this)
        setContentView(ScrollView(this).apply { addView(content) })
        if (top < 0) showLineup() else showRelay()
        handler.post(ticker)
    }

    override fun onSupportNavigateUp(): Boolean { finish(); return true }
    override fun onDestroy() { handler.removeCallbacksAndMessages(null); super.onDestroy() }
    override fun onStop() { if (clock.running) { clock.pause(now()) }; save(); super.onStop() }

    // ------------------------------------------------------------ journal

    private fun restore(log: JSONObject) {
        top = log.optInt("top", -1).takeIf { log.has("top") } ?: -1
        if (log.optJSONArray("lineup") == null && log.optJSONArray("relays") == null) top = -1
        revision = log.optLong("revision")
        lineup = (0 until (log.optJSONArray("lineup")?.length() ?: 0)).map { log.getJSONArray("lineup").optString(it) }.toMutableList()
        relays.clear()
        log.optJSONArray("relays")?.let { a -> for (i in 0 until a.length()) { val r = a.getJSONArray(i); relays.add(intArrayOf(r.optInt(0), r.optInt(1))) } }
        winner = if (log.isNull("winner")) -1 else log.optInt("winner", -1)
        relays.lastOrNull()?.let { current[0] = it[0]; current[1] = it[1] }
    }

    private fun log(): JSONObject = JSONObject()
        .put("revision", revision)
        .put("format", format.json)
        .put("top", top.coerceAtLeast(0))
        .put("lineup", JSONArray(lineup))
        .put("relays", JSONArray().apply { relays.forEach { put(JSONArray().put(it[0]).put(it[1])) } })
        .put("winner", if (winner >= 0) winner else JSONObject.NULL)

    private fun save() { cache().edit().putString(cacheKey, log().toString()).apply() }

    /** Journal envoyé au serveur (sans valider le score) après chaque relais. */
    private fun sync() {
        revision++
        save()
        api.relay(token, bout, log(), null) { }
    }

    // --------------------------------------------------------- composition

    private fun showLineup() {
        content.removeAllViews()
        content.addView(Ui.text(this, "Composition de la rencontre", 22f, true))
        content.addView(Ui.text(this, format.label(), 14f, color = Ui.MUTED))
        content.addView(Ui.space(this, 12))
        content.addView(Ui.text(this, "Quelle équipe est en haut (${format.numbers(0).joinToString("-")}) ?", 17f, true))
        val choice = Ui.row(this)
        val buttons = (0..1).map { k ->
            Ui.button(this, teams[k], primary = top == k) { top = k; showLineup() }.apply {
                layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginEnd = Ui.dp(this@TeamBoutActivity, 6) }
            }
        }
        buttons.forEach { choice.addView(it) }
        content.addView(choice)
        if (top < 0) return
        content.addView(Ui.space(this, 12))
        val spinners = mutableListOf<Pair<Int, Spinner>>()
        for (side in 0..1) {
            val team = if (side == 0) top else 1 - top
            content.addView(Ui.text(this, (if (side == 0) "En haut : " else "En bas : ") + teams[team], 17f, true, Ui.BLUE))
            for (n in format.numbers(side)) {
                val row = Ui.row(this)
                row.addView(Ui.text(this, "$n.", 18f, true).apply { minWidth = Ui.dp(this@TeamBoutActivity, 36) })
                val options = listOf("—") + members[team]
                val spinner = Spinner(this).apply {
                    adapter = ArrayAdapter(this@TeamBoutActivity, android.R.layout.simple_spinner_dropdown_item, options)
                    layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
                    val known = lineup.getOrNull(n - 1)
                    options.indexOf(known).takeIf { it > 0 }?.let { setSelection(it) }
                }
                row.addView(spinner)
                content.addView(row)
                spinners.add(n to spinner)
            }
            content.addView(Ui.space(this, 8))
        }
        content.addView(Ui.text(this, "Le choix des noms est facultatif : seuls les numéros comptent pour l'ordre des relais.", 13f, color = Ui.MUTED))
        content.addView(Ui.space(this, 12))
        content.addView(Ui.button(this, "Commencer la rencontre") {
            val size = 2 * format.teamSize
            lineup = MutableList(size) { "" }
            for ((n, spinner) in spinners) {
                val v = spinner.selectedItem?.toString() ?: ""
                lineup[n - 1] = if (v == "—") "" else v
            }
            sync()
            showRelay()
        })
    }

    // -------------------------------------------------------------- relais

    private fun name(n: Int) = lineup.getOrNull(n - 1)?.takeIf { it.isNotEmpty() } ?: ""

    private fun over(): Boolean = relays.size >= format.relays || relays.lastOrNull()?.let { it[0] >= format.total || it[1] >= format.total } == true

    private fun showRelay() {
        content.removeAllViews()
        val topTeam = teams[top]
        val bottomTeam = teams[1 - top]
        if (over()) return showEnd()
        val index = relays.size
        val (a, b) = format.bouts[index]
        val target = format.target(index)
        val previous = relays.lastOrNull() ?: intArrayOf(0, 0)
        content.addView(Ui.text(this, "Relais ${index + 1} / ${format.relays}", 22f, true))
        content.addView(Ui.text(this, "$a. ${name(a)}  contre  $b. ${name(b)}".replace("  ", " ").trim(), 18f, true, Ui.BLUE))
        content.addView(Ui.text(this, "Objectif : $target touches (ou fin du temps)", 15f, color = Ui.MUTED))
        content.addView(Ui.space(this, 8))
        val board = Ui.row(this)
        for (side in 0..1) {
            val panel = Ui.card(this).apply { layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginEnd = Ui.dp(this@TeamBoutActivity, 4) } }
            panel.addView(Ui.text(this, if (side == 0) topTeam else bottomTeam, 15f, true).apply { gravity = Gravity.CENTER })
            val score = Ui.text(this, current[side].toString(), 54f, true, if (side == 0) 0xFFE53935.toInt() else 0xFF00A152.toInt()).apply { gravity = Gravity.CENTER }
            panel.addView(score)
            val row = Ui.row(this).apply { gravity = Gravity.CENTER }
            row.addView(Ui.button(this, "−", primary = false) {
                current[side] = (current[side] - 1).coerceAtLeast(previous[side]); score.text = current[side].toString()
            })
            row.addView(Ui.button(this, "+") {
                current[side] = (current[side] + 1).coerceAtMost(target); score.text = current[side].toString()
                if (current[side] >= target && clock.running) clock.pause(now())
            })
            panel.addView(row)
            board.addView(panel)
        }
        content.addView(board)
        content.addView(Ui.space(this, 8))
        timerView = Ui.text(this, clock.display(now()), 44f, true).apply { gravity = Gravity.CENTER }
        content.addView(timerView)
        toggle = Ui.button(this, if (clock.running) "Pause" else "Démarrer le chrono") {
            if (clock.running) clock.pause(now()) else clock.start(now())
            toggle?.text = if (clock.running) "Pause" else "Démarrer le chrono"
        }
        content.addView(toggle)
        content.addView(Ui.space(this, 8))
        content.addView(Ui.button(this, "Fin du relais ${index + 1}") {
            val reached = current[0] >= target || current[1] >= target
            val finish = {
                clock.pause(now())
                relays.add(intArrayOf(current[0], current[1]))
                clock.reset(now())
                sync()
                showRelay()
            }
            if (reached || clock.remaining(now()) == 0L) finish()
            else AlertDialog.Builder(this).setMessage("L'objectif de $target n'est pas atteint. Terminer le relais au temps sur ${current[0]} – ${current[1]} ?")
                .setPositiveButton("Terminer") { _, _ -> finish() }.setNegativeButton("Annuler", null).show()
        })
        if (relays.isNotEmpty()) {
            content.addView(Ui.button(this, "Corriger le relais précédent", primary = false) {
                relays.removeAt(relays.size - 1)
                val p = relays.lastOrNull() ?: intArrayOf(0, 0)
                current[0] = maxOf(current[0], p[0]).coerceAtMost(format.target(relays.size)); current[1] = maxOf(current[1], p[1]).coerceAtMost(format.target(relays.size))
                sync(); showRelay()
            })
        }
        history()
    }

    private fun history() {
        if (relays.isEmpty()) return
        content.addView(Ui.space(this, 12))
        content.addView(Ui.text(this, "Relais terminés", 16f, true))
        relays.forEachIndexed { i, r ->
            val (a, b) = format.bouts[i]
            content.addView(Ui.text(this, "${i + 1}. $a-$b  ·  ${r[0]} – ${r[1]}", 15f))
        }
    }

    private fun showEnd() {
        val last = relays.last()
        content.addView(Ui.text(this, "Fin de la rencontre", 22f, true))
        content.addView(Ui.text(this, "${teams[top]} ${last[0]}  –  ${last[1]} ${teams[1 - top]}", 20f, true, Ui.BLUE))
        if (last[0] == last[1]) {
            content.addView(Ui.space(this, 8))
            content.addView(Ui.text(this, "Égalité : désignez l'équipe gagnante.", 16f, true, Ui.RED))
            val row = Ui.row(this)
            for (k in 0..1) row.addView(Ui.button(this, teams[k], primary = winner == k) { winner = k; save(); content.removeAllViews(); showEnd() }.apply {
                layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
            })
            content.addView(row)
        }
        content.addView(Ui.space(this, 12))
        val status = Ui.text(this, "", 15f)
        content.addView(Ui.button(this, "Envoyer le résultat") { submit(status) })
        content.addView(status)
        content.addView(Ui.button(this, "Corriger le dernier relais", primary = false) {
            relays.removeAt(relays.size - 1); winner = -1; sync(); showRelay()
        })
        history()
    }

    /** Scores finaux dans l'ordre de la feuille (équipe A, équipe B). */
    private fun values(): List<String>? {
        val last = relays.lastOrNull() ?: return null
        val bySheet = if (top == 0) intArrayOf(last[0], last[1]) else intArrayOf(last[1], last[0])
        val w = when { bySheet[0] > bySheet[1] -> 0; bySheet[1] > bySheet[0] -> 1; else -> winner }
        if (w < 0) return null
        return (0..1).map { if (it == w) "W${bySheet[it]}" else "${bySheet[it]}" }
    }

    private fun submit(status: TextView) {
        if (sending) return
        val values = values() ?: run { status.text = "Désignez l'équipe gagnante."; return }
        sending = true
        revision++
        save()
        status.setTextColor(Ui.MUTED); status.text = "Envoi…"
        api.relay(token, bout, log(), values) { result ->
            result.onSuccess { answer ->
                when (answer.optString("status")) {
                    "pending" -> { status.text = "En attente de ManageComp…"; poll(answer.optLong("id"), 0, status) }
                    else -> done(status)
                }
            }.onFailure { sending = false; status.setTextColor(Ui.RED); status.text = it.message ?: "Erreur" }
        }
    }

    private fun poll(id: Long, attempt: Int, status: TextView) {
        if (attempt > 40) { sending = false; status.text = "Résultat transmis ; ManageComp l'intégrera dès qu'il sera connecté."; return }
        handler.postDelayed({
            api.result(id) { r ->
                r.onSuccess { a -> when (a.optString("status")) {
                    "ok" -> done(status)
                    "error" -> { sending = false; status.setTextColor(Ui.RED); status.text = a.optString("error") }
                    else -> poll(id, attempt + 1, status)
                } }.onFailure { poll(id, attempt + 1, status) }
            }
        }, 1500)
    }

    private fun done(status: TextView) {
        cache().edit().remove(cacheKey).apply()
        if (piste > 0) api.live(liveState(true)) { }
        status.setTextColor(Ui.GREEN); status.text = "Résultat enregistré ✔"
        handler.postDelayed({ finish() }, 900)
    }

    // ------------------------------------------------------- écran de piste

    private fun liveState(over: Boolean): JSONObject {
        val index = relays.size.coerceAtMost(format.relays - 1)
        val (a, b) = format.bouts[index]
        val scores = if (relays.size >= format.relays) relays.last() else current
        return JSONObject()
            .put("piste", piste).put("competition", competition).put("color", color)
            .put("bout", "Relais ${index + 1}/${format.relays} : $a-$b")
            .put("red", JSONObject().put("name", teams[top.coerceAtLeast(0)]).put("score", scores[0].toString()))
            .put("green", JSONObject().put("name", teams[1 - top.coerceAtLeast(0)]).put("score", scores[1].toString()))
            .put("remaining_ms", clock.remaining(now())).put("running", clock.running)
            .put("period", index + 1).put("periods", format.relays).put("over", over)
    }

    private fun pushLive() {
        if (piste <= 0 || top < 0 || liveInFlight) return
        val key = "${relays.size}|${current[0]}|${current[1]}|${clock.running}|" + if (clock.running) "" else clock.remaining(now())
        if (key == liveKey && now() - liveAt < (if (clock.running) 3000 else 20000)) return
        liveKey = key; liveAt = now(); liveInFlight = true
        api.live(liveState(false)) { liveInFlight = false }
    }
}

/** Format des relais reçu de ManageComp (`relay` de la feuille). */
class RelayFormat(val json: JSONObject) {
    val single = json.optString("kind") == "single"
    val teamSize = json.optInt("team_size", 3).coerceIn(1, 20)
    val points = json.optInt("points", 5).coerceIn(1, 15)
    val bouts: List<Pair<Int, Int>> = when {
        single -> (0 until teamSize).map { 2 * it + 1 to 2 * it + 2 }
        teamSize == 2 -> listOf(1 to 4, 2 to 3, 1 to 3, 2 to 4)
        teamSize == 3 -> listOf(3 to 6, 1 to 5, 2 to 4, 1 to 6, 3 to 4, 2 to 5, 1 to 4, 2 to 6, 3 to 5)
        else -> (0 until teamSize).flatMap { r -> (0 until teamSize).map { i -> (i + 1) to (teamSize + 1 + (i + r) % teamSize) } }
    }
    val relays get() = bouts.size
    fun target(index: Int) = points * (index + 1)
    val total get() = target(relays - 1)
    fun numbers(side: Int): List<Int> =
        if (single) (0 until teamSize).map { 2 * it + 1 + side } else (1..teamSize).map { it + side * teamSize }
    fun label() = if (single) "Relais simple, équipes de $teamSize, relais de $points points ($total touches)"
        else "$teamSize contre $teamSize, chacun contre chacun, relais de $points points ($total touches)"
}
