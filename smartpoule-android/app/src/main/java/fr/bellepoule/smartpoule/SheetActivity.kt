package fr.bellepoule.smartpoule

import android.content.Intent
import android.graphics.Color
import android.graphics.Typeface
import android.os.Bundle
import android.view.Gravity
import android.widget.HorizontalScrollView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TableLayout
import android.widget.TableRow
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import org.json.JSONObject

/**
 * Feuille d'une poule (grille des résultats, puis les matchs dans l'ordre)
 * ou d'un match de tableau. Un match touché ouvre la saisie des touches.
 */
class SheetActivity : AppCompatActivity() {
    private lateinit var api: Api
    private lateinit var token: String
    private lateinit var content: LinearLayout

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val pairing = Pairing.load(this) ?: return finish()
        api = Api(pairing)
        token = intent.getStringExtra("token") ?: return finish()
        content = Ui.column(this)
        setContentView(ScrollView(this).apply { addView(content) })
        supportActionBar?.setDisplayHomeAsUpEnabled(true)
    }

    override fun onSupportNavigateUp(): Boolean {
        finish()
        return true
    }

    override fun onResume() {
        super.onResume()
        api.sheet(token) { result ->
            result.onSuccess { show(it) }.onFailure {
                content.removeAllViews()
                content.addView(Ui.text(this, it.message ?: "Erreur", color = Ui.RED))
            }
        }
    }

    private fun show(sheet: JSONObject) {
        title = sheet.optString("name")
        content.removeAllViews()
        content.addView(Ui.text(this, sheet.optString("competition"), 13f, color = Ui.MUTED))
        sheet.optString("piste").takeIf { it.isNotEmpty() }?.let {
            content.addView(Ui.text(this, "Piste $it", 18f, true, Ui.BLUE))
        }
        content.addView(Ui.space(this, 8))
        if (sheet.optString("kind") == "pool") {
            content.addView(grid(sheet))
            content.addView(Ui.space(this, 16))
            content.addView(Ui.text(this, "Matchs", 20f, true))
            content.addView(Ui.space(this, 6))
        }
        val max = sheet.optInt("max_score", 5)
        val bouts = sheet.optJSONArray("bouts")
        if (bouts == null || bouts.length() == 0) {
            content.addView(Ui.text(this, "Aucun match à saisir pour l'instant : la composition des poules n'est pas encore validée dans ManageComp.", 16f, color = Ui.MUTED))
            return
        }
        for (i in 0 until bouts.length()) {
            val bout = bouts.getJSONObject(i)
            val fencers = bout.optJSONArray("fencers") ?: continue
            if (fencers.length() < 2) continue
            val a = fencers.getJSONObject(0)
            val b = fencers.getJSONObject(1)
            val card = Ui.card(this)
            val head = Ui.row(this)
            head.addView(Ui.text(this, bout.optString("name"), 14f, true, Ui.MUTED).apply {
                layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
            })
            bout.optString("resource").takeIf { it.isNotEmpty() && it != "null" }?.let { head.addView(Ui.text(this, it, 13f, color = Ui.MUTED)) }
            if (bout.optBoolean("over")) head.addView(Ui.text(this, "  ✔", 16f, true, Ui.GREEN))
            card.addView(head)
            card.addView(line(a))
            card.addView(line(b))
            bout.optJSONObject("officiating")?.optJSONArray("cards")?.let { cards ->
                for (ci in 0 until cards.length()) {
                    val event = cards.getJSONObject(ci)
                    val who = if (event.optInt("side") == 0) a.optString("name") else b.optString("name")
                    val color = when(event.optString("color")) { "yellow" -> "jaune"; "red" -> "rouge"; else -> "noir" }
                    val seconds = (event.optLong("remaining_ms") + 999) / 1000
                    card.addView(Ui.text(this, "$who : $color · M${event.optInt("period")} %02d:%02d".format(seconds / 60, seconds % 60) + if (event.optBoolean("cancelled")) " (annulé)" else "", 12f, color = Ui.MUTED))
                }
            }
            val relay = sheet.optJSONObject("relay")
            card.setOnClickListener {
                // Compétition par équipes : rencontre en relais.
                if (relay != null) {
                    startActivity(
                        Intent(this, TeamBoutActivity::class.java)
                            .putExtra("token", token)
                            .putExtra("bout", bout.optInt("index"))
                            .putExtra("relay", relay.toString())
                            .putExtra("timing", (sheet.optJSONObject("timing") ?: JSONObject()).toString())
                            .putExtra("relay_log", bout.optJSONObject("relay_log")?.toString())
                            .putExtra("piste", bout.optInt("piste"))
                            .putExtra("competition", sheet.optString("competition"))
                            .putExtra("color", sheet.optString("color"))
                            .putExtra("title", sheet.optString("name") + " · " + bout.optString("name"))
                            .putExtra("a", a.toString())
                            .putExtra("b", b.toString())
                    )
                    return@setOnClickListener
                }
                startActivity(
                    Intent(this, BoutActivity::class.java)
                        .putExtra("token", token)
                        .putExtra("bout", bout.optInt("index"))
                        .putExtra("max", max)
                        .putExtra("timing", (sheet.optJSONObject("timing") ?: JSONObject().put("period_seconds", 180).put("periods", if (sheet.optString("kind") == "match") 3 else 1)).toString())
                        .putExtra("officiating", bout.optJSONObject("officiating")?.toString())
                        .putExtra("over", bout.optBoolean("over"))
                        .putExtra("piste", bout.optInt("piste"))
                        .putExtra("competition", sheet.optString("competition"))
                        .putExtra("color", sheet.optString("color"))
                        .putExtra("title", sheet.optString("name") + " · " + bout.optString("name"))
                        .putExtra("a", a.toString())
                        .putExtra("b", b.toString())
                )
            }
            content.addView(card)
        }
    }

    /** Ligne d'un tireur : numéro, nom, club, score. */
    private fun line(f: JSONObject): LinearLayout {
        val row = Ui.row(this)
        val position = f.optInt("position")
        row.addView(Ui.text(this, if (position > 0) "$position" else "", 16f, true, Ui.MUTED).apply {
            minWidth = Ui.dp(this@SheetActivity, 26)
        })
        val who = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
        }
        who.addView(Ui.text(this, f.optString("name"), 17f, true))
        f.optString("club").takeIf { it.isNotEmpty() }?.let { who.addView(Ui.text(this, it, 13f, color = Ui.MUTED)) }
        row.addView(who)
        row.addView(Ui.text(this, f.optString("score"), 22f, true, Ui.BLUE))
        return row
    }

    /** Grille de la poule (lecture seule). */
    private fun grid(sheet: JSONObject): HorizontalScrollView {
        val table = TableLayout(this)
        val fencers = sheet.optJSONArray("fencers") ?: return HorizontalScrollView(this)
        val n = fencers.length()
        val pad = Ui.dp(this, 6)
        fun cell(text: String, bold: Boolean = false, grey: Boolean = false, color: Int = Ui.INK) = TextView(this).apply {
            this.text = text
            gravity = Gravity.CENTER
            setPadding(pad, pad, pad, pad)
            setTextColor(color)
            if (bold) setTypeface(typeface, Typeface.BOLD)
            setBackgroundColor(if (grey) 0xFFBDBDBD.toInt() else Color.WHITE)
        }
        val header = TableRow(this)
        header.addView(cell(""))
        header.addView(cell(""))
        for (j in 1..n) header.addView(cell("$j", true, color = Ui.MUTED))
        header.addView(cell("V", true, color = Ui.MUTED))
        header.addView(cell("Ind.", true, color = Ui.MUTED))
        header.addView(cell("Pl.", true, color = Ui.MUTED))
        table.addView(header)
        for (i in 0 until n) {
            val f = fencers.getJSONObject(i)
            val row = TableRow(this)
            row.addView(cell(f.optString("name"), true).apply { gravity = Gravity.START })
            row.addView(cell(f.optString("position"), true, color = Ui.MUTED))
            val cells = f.optJSONArray("cells")
            for (j in 0 until n) {
                val v = cells?.opt(j)
                val grey = v == null || v == JSONObject.NULL
                row.addView(cell(if (grey) "" else v.toString(), grey = grey))
            }
            row.addView(cell(f.optString("victories").takeIf { it != "null" } ?: ""))
            row.addView(cell(f.optString("indice").takeIf { it != "null" } ?: ""))
            row.addView(cell(f.optString("rank").takeIf { it != "null" } ?: "", true))
            table.addView(row)
        }
        table.setBackgroundColor(0xFFDCE5F4.toInt())
        table.setPadding(1, 1, 1, 1)
        return HorizontalScrollView(this).apply { addView(table) }
    }
}
