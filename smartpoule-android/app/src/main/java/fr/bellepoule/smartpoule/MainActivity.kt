package fr.bellepoule.smartpoule

import android.content.Intent
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.text.InputType
import android.view.Menu
import android.view.MenuItem
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import org.json.JSONObject

/**
 * Accueil : appairage par QR code, puis liste des matchs de l'arbitre
 * (piste, heure, état), actualisée régulièrement.
 */
class MainActivity : AppCompatActivity() {
    private var pairing: Pairing? = null
    private var api: Api? = null
    private lateinit var content: LinearLayout
    private val handler = Handler(Looper.getMainLooper())
    private val refresh = object : Runnable {
        override fun run() {
            load()
            handler.postDelayed(this, 15000)
        }
    }

    private val scan = registerForActivityResult(ScanContract()) { result ->
        result.contents?.let { pair(it) }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        content = Ui.column(this)
        setContentView(ScrollView(this).apply { addView(content) })
        pairing = Pairing.load(this)
        api = pairing?.let { Api(it) }
    }

    override fun onResume() {
        super.onResume()
        if (pairing == null) showPairing() else handler.post(refresh)
    }

    override fun onPause() {
        super.onPause()
        handler.removeCallbacks(refresh)
    }

    override fun onCreateOptionsMenu(menu: Menu): Boolean {
        menu.add(0, 1, 0, "Actualiser")
        menu.add(0, 2, 1, "Changer de QR code")
        return true
    }

    override fun onOptionsItemSelected(item: MenuItem): Boolean {
        when (item.itemId) {
            1 -> load()
            2 -> {
                Pairing.clear(this)
                pairing = null
                api = null
                handler.removeCallbacks(refresh)
                showPairing()
            }
        }
        return true
    }

    // --------------------------------------------------------- appairage

    private fun showPairing() {
        title = "SmartPoule"
        content.removeAllViews()
        content.addView(Ui.text(this, "Bienvenue", 26f, true))
        content.addView(Ui.space(this, 8))
        content.addView(Ui.text(this, "Scannez le QR code d'arbitre que vous a remis l'organisation (BellePoule › BellePoule2D › SmartPoule…).", 16f, color = Ui.MUTED))
        content.addView(Ui.space(this, 24))
        content.addView(Ui.button(this, "Scanner le QR code") {
            scan.launch(ScanOptions().apply {
                setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                setPrompt("Visez le QR code de l'arbitre")
                setBeepEnabled(false)
                setOrientationLocked(false)
            })
        })
        content.addView(Ui.space(this, 12))
        content.addView(Ui.button(this, "Saisir l'adresse à la main", primary = false) {
            val input = EditText(this).apply {
                inputType = InputType.TYPE_TEXT_VARIATION_URI
                hint = "https://…/smartpoule/?c=…&r=…&k=…"
            }
            AlertDialog.Builder(this)
                .setTitle("Adresse du QR code")
                .setView(input)
                .setPositiveButton("Valider") { _, _ -> pair(input.text.toString()) }
                .setNegativeButton("Annuler", null)
                .show()
        })
    }

    private fun pair(text: String) {
        val p = Pairing.parse(text)
        if (p == null) {
            alert("Ce QR code n'est pas un QR code SmartPoule.")
            return
        }
        val candidate = Api(p)
        candidate.jobs { result ->
            result.onSuccess {
                p.save(this)
                pairing = p
                api = candidate
                show(it)
                handler.postDelayed(refresh, 15000)
            }.onFailure { alert(it.message ?: "Erreur") }
        }
    }

    // ----------------------------------------------------------- matchs

    private fun load() {
        api?.jobs { result ->
            result.onSuccess { show(it) }.onFailure { error(it.message ?: "Erreur") }
        }
    }

    private fun error(message: String) {
        if (content.childCount == 0) {
            content.addView(Ui.text(this, message, color = Ui.RED))
        } else {
            title = "SmartPoule · hors ligne"
        }
    }

    private fun show(data: JSONObject) {
        val name = data.optString("name")
        title = name.ifEmpty { "SmartPoule" }
        content.removeAllViews()
        content.addView(Ui.text(this, data.optString("club"), 14f, color = Ui.MUTED))
        content.addView(Ui.text(this, if (pairing?.isAdmin == true) "Tous les matchs" else "Mes matchs", 24f, true))
        content.addView(Ui.space(this, 12))
        val jobs = data.optJSONArray("jobs")
        if (jobs == null || jobs.length() == 0) {
            content.addView(Ui.text(this, "Aucun match pour l'instant. Cette liste se met à jour toute seule quand l'organisation vous affecte une poule ou un match.", 16f, color = Ui.MUTED))
            return
        }
        // Les matchs à arbitrer d'abord, par heure.
        val list = (0 until jobs.length()).map { jobs.getJSONObject(it) }
            .sortedWith(compareBy({ it.optBoolean("over") }, { it.optString("time").ifEmpty { "99:99" } }))
        for (job in list) {
            val card = Ui.card(this)
            val head = Ui.row(this)
            val title = Ui.text(this, job.optString("name"), 18f, true).apply {
                layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
            }
            head.addView(title)
            if (job.optBoolean("over")) head.addView(Ui.text(this, "✔ terminé", 14f, true, Ui.GREEN))
            card.addView(head)
            val where = listOfNotNull(
                job.optString("piste").takeIf { it.isNotEmpty() }?.let { "Piste $it" },
                job.optString("time").takeIf { it.isNotEmpty() }?.let { "à $it" },
            ).joinToString(" · ")
            if (where.isNotEmpty()) card.addView(Ui.text(this, where, 17f, true, Ui.BLUE))
            job.optString("summary").takeIf { it.isNotEmpty() }?.let { card.addView(Ui.text(this, it, 15f)) }
            card.addView(Ui.text(this, job.optString("competition"), 13f, color = Ui.MUTED))
            card.setOnClickListener {
                startActivity(Intent(this, SheetActivity::class.java).putExtra("token", job.optString("token")))
            }
            content.addView(card)
        }
    }

    private fun alert(message: String) {
        AlertDialog.Builder(this).setMessage(message).setPositiveButton("OK", null).show()
    }
}
