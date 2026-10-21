package fr.bellepoule.smartpoule

import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import org.json.JSONObject

/**
 * Saisie d'un match : un compteur de touches par tireur (+ / −), le choix
 * du vainqueur quand les scores sont égaux, puis l'envoi à BellePoule.
 */
class BoutActivity : AppCompatActivity() {
    private lateinit var api: Api
    private lateinit var token: String
    private var bout = 0
    private var max = 5
    private val scores = intArrayOf(0, 0)
    private var winner = -1
    private val names = arrayOf("", "")
    private val counters = arrayOfNulls<TextView>(2)
    private val panels = arrayOfNulls<LinearLayout>(2)
    private lateinit var status: TextView
    private lateinit var send: android.widget.Button
    private val handler = Handler(Looper.getMainLooper())

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val pairing = Pairing.load(this) ?: return finish()
        api = Api(pairing)
        token = intent.getStringExtra("token") ?: return finish()
        bout = intent.getIntExtra("bout", 0)
        max = intent.getIntExtra("max", 5)
        title = intent.getStringExtra("title") ?: "Match"
        supportActionBar?.setDisplayHomeAsUpEnabled(true)

        val content = Ui.column(this)
        content.addView(Ui.text(this, "Match en $max touches · touchez un tireur pour le désigner vainqueur en cas d'égalité", 14f, color = Ui.MUTED))
        content.addView(Ui.space(this, 12))
        for (k in 0..1) {
            val f = JSONObject(intent.getStringExtra(if (k == 0) "a" else "b") ?: "{}")
            names[k] = f.optString("name")
            scores[k] = parse(f.optString("score"))
            if (f.optString("score").trim().uppercase().let { it.startsWith("V") || it.startsWith("W") }) winner = k
            content.addView(panel(k, f))
            content.addView(Ui.space(this, 12))
        }
        status = Ui.text(this, "", 15f).apply { gravity = Gravity.CENTER }
        content.addView(status)
        content.addView(Ui.space(this, 8))
        send = Ui.button(this, "Envoyer le score") { confirm() }
        content.addView(send)
        setContentView(ScrollView(this).apply { addView(content) })
        refresh()
    }

    override fun onSupportNavigateUp(): Boolean {
        finish()
        return true
    }

    override fun onDestroy() {
        super.onDestroy()
        handler.removeCallbacksAndMessages(null)
    }

    /** Score déjà connu (« V », « W4 », « 3 »…). */
    private fun parse(value: String): Int {
        val v = value.trim().uppercase()
        if (v == "V") return max
        return v.trimStart('V', 'W').toIntOrNull() ?: 0
    }

    private fun panel(k: Int, f: JSONObject): LinearLayout {
        val panel = Ui.column(this, 14)
        panels[k] = panel
        val lp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        panel.layoutParams = lp
        val position = f.optInt("position")
        panel.addView(Ui.text(this, (if (position > 0) "$position · " else "") + f.optString("name"), 20f, true))
        f.optString("club").takeIf { it.isNotEmpty() }?.let { panel.addView(Ui.text(this, it, 14f, color = Ui.MUTED)) }
        val row = Ui.row(this).apply { gravity = Gravity.CENTER }
        val minus = Ui.button(this, "−", primary = false) { change(k, -1) }.apply { textSize = 30f }
        val plus = Ui.button(this, "+") { change(k, +1) }.apply { textSize = 30f }
        val size = Ui.dp(this, 72)
        minus.layoutParams = LinearLayout.LayoutParams(size, size)
        plus.layoutParams = LinearLayout.LayoutParams(size, size)
        val counter = Ui.text(this, "0", 56f, true, Ui.BLUE).apply {
            gravity = Gravity.CENTER
            minWidth = Ui.dp(this@BoutActivity, 120)
        }
        counters[k] = counter
        row.addView(minus)
        row.addView(counter)
        row.addView(plus)
        panel.addView(row)
        panel.setOnClickListener {
            winner = k
            refresh()
        }
        return panel
    }

    private fun change(k: Int, delta: Int) {
        scores[k] = (scores[k] + delta).coerceIn(0, max)
        refresh()
    }

    /** Vainqueur déduit des scores, ou choisi à la main en cas d'égalité. */
    private fun currentWinner(): Int = when {
        scores[0] > scores[1] -> 0
        scores[1] > scores[0] -> 1
        else -> winner
    }

    private fun refresh() {
        val w = currentWinner()
        for (k in 0..1) {
            counters[k]?.text = scores[k].toString()
            panels[k]?.background = Ui.rounded(
                if (k == w) 0xFFE6F4EC.toInt() else Ui.CARD,
                if (k == w) Ui.GREEN else 0xFFDCE5F4.toInt(),
                12f, this
            )
        }
        status.setTextColor(Ui.MUTED)
        status.text = when {
            w < 0 -> "Égalité : touchez le tireur vainqueur."
            else -> "Vainqueur : ${names[w]}"
        }
    }

    private fun confirm() {
        val w = currentWinner()
        if (w < 0) {
            status.setTextColor(Ui.RED)
            status.text = "Égalité : touchez le tireur vainqueur avant d'envoyer."
            return
        }
        val text = "${names[0]}  ${scores[0]}\n${names[1]}  ${scores[1]}\n\nVainqueur : ${names[w]}"
        AlertDialog.Builder(this)
            .setTitle("Envoyer ce score ?")
            .setMessage(text)
            .setPositiveButton("Envoyer") { _, _ -> submit(w) }
            .setNegativeButton("Annuler", null)
            .show()
    }

    private fun submit(w: Int) {
        val values = (0..1).map { if (it == w) "W${scores[it]}" else "${scores[it]}" }
        send.isEnabled = false
        status.setTextColor(Ui.MUTED)
        status.text = "Envoi…"
        api.score(token, bout, values[0], values[1]) { result ->
            result.onSuccess { answer ->
                when (answer.optString("status")) {
                    "pending" -> {
                        status.text = "En attente de BellePoule…"
                        poll(answer.optLong("id"), 0)
                    }
                    "error" -> failed(answer.optString("error"))
                    else -> done()
                }
            }.onFailure { failed(it.message ?: "Erreur") }
        }
    }

    /** BellePoule relève les scores toutes les quelques secondes. */
    private fun poll(id: Long, attempt: Int) {
        if (attempt > 40) {
            status.setTextColor(Ui.MUTED)
            status.text = "Score transmis au serveur ; BellePoule l'intégrera dès qu'il sera connecté."
            send.isEnabled = true
            return
        }
        handler.postDelayed({
            api.result(id) { result ->
                result.onSuccess { answer ->
                    when (answer.optString("status")) {
                        "ok" -> done()
                        "error" -> failed(answer.optString("error"))
                        else -> poll(id, attempt + 1)
                    }
                }.onFailure { poll(id, attempt + 1) }
            }
        }, 1500)
    }

    private fun done() {
        status.setTextColor(Ui.GREEN)
        status.text = "Score enregistré ✔"
        handler.postDelayed({ finish() }, 900)
    }

    private fun failed(message: String) {
        send.isEnabled = true
        status.setTextColor(Ui.RED)
        status.text = message.ifEmpty { "Score refusé" }
    }
}
