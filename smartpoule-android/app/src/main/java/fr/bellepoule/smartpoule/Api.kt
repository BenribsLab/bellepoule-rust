package fr.bellepoule.smartpoule

import android.content.Context
import android.net.Uri
import android.os.Handler
import android.os.Looper
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import java.util.concurrent.Executors

/**
 * Identité de l'arbitre, lue dans son QR code :
 * `https://serveur/smartpoule/?c=<club>&r=<arbitre>&k=<clé>`.
 */
data class Pairing(val server: String, val club: String, val referee: Int, val key: String) {
    companion object {
        /** Lit le texte d'un QR code SmartPoule (null s'il ne convient pas). */
        fun parse(text: String): Pairing? {
            val uri = try { Uri.parse(text.trim()) } catch (e: Exception) { return null }
            val scheme = uri.scheme ?: return null
            if (scheme != "http" && scheme != "https") return null
            val club = uri.getQueryParameter("c") ?: return null
            val referee = uri.getQueryParameter("r")?.toIntOrNull() ?: return null
            val key = uri.getQueryParameter("k") ?: return null
            // Adresse du serveur : tout ce qui précède « /smartpoule ».
            val path = uri.path ?: ""
            val prefix = path.substringBefore("/smartpoule").trimEnd('/')
            val port = if (uri.port > 0) ":${uri.port}" else ""
            return Pairing("$scheme://${uri.host}$port$prefix", club, referee, key)
        }

        fun load(context: Context): Pairing? {
            val p = context.getSharedPreferences("smartpoule", Context.MODE_PRIVATE)
            val server = p.getString("server", null) ?: return null
            return Pairing(server, p.getString("club", "")!!, p.getInt("referee", -1), p.getString("key", "")!!)
        }

        fun clear(context: Context) {
            context.getSharedPreferences("smartpoule", Context.MODE_PRIVATE).edit().clear().apply()
        }
    }

    fun save(context: Context) {
        context.getSharedPreferences("smartpoule", Context.MODE_PRIVATE).edit()
            .putString("server", server)
            .putString("club", club)
            .putInt("referee", referee)
            .putString("key", key)
            .apply()
    }

    val isAdmin: Boolean get() = referee == 0
}

/** Erreur lisible renvoyée par le serveur (`{ "error": … }`). */
class ApiException(message: String) : Exception(message)

/**
 * Appels au serveur BellePoule Web (relais vers BellePoule). Les réponses
 * sont rendues sur le fil principal.
 */
class Api(private val pairing: Pairing) {
    private val executor = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())

    private fun enc(s: String) = URLEncoder.encode(s, "UTF-8")

    private fun identity() = "c=${enc(pairing.club)}&r=${pairing.referee}&k=${enc(pairing.key)}"

    private fun request(method: String, path: String, body: String?): String {
        val conn = URL(pairing.server + path).openConnection() as HttpURLConnection
        conn.requestMethod = method
        conn.connectTimeout = 10000
        conn.readTimeout = 15000
        conn.setRequestProperty("Accept", "application/json")
        if (body != null) {
            conn.doOutput = true
            conn.setRequestProperty("Content-Type", "application/json; charset=utf-8")
            conn.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
        }
        val code = conn.responseCode
        val stream = if (code in 200..299) conn.inputStream else conn.errorStream
        val text = stream?.bufferedReader(Charsets.UTF_8)?.use { it.readText() } ?: ""
        if (code !in 200..299) {
            val message = try { JSONObject(text).optString("error") } catch (e: Exception) { "" }
            throw ApiException(if (message.isNotEmpty()) message else "Erreur du serveur ($code)")
        }
        return text
    }

    private fun <T> run(work: () -> T, done: (Result<T>) -> Unit) {
        executor.execute {
            val result = try {
                Result.success(work())
            } catch (e: ApiException) {
                Result.failure(e)
            } catch (e: IOException) {
                Result.failure(ApiException("Serveur injoignable : vérifiez la connexion Internet."))
            } catch (e: Exception) {
                Result.failure(ApiException(e.message ?: "Erreur"))
            }
            main.post { done(result) }
        }
    }

    /** Matchs de l'arbitre : `{ club, name, jobs: [...] }`. */
    fun jobs(done: (Result<JSONObject>) -> Unit) =
        run({ JSONObject(request("GET", "/api/smartpoule/phone/jobs?${identity()}", null)) }, done)

    /** Feuille d'une poule ou d'un match. */
    fun sheet(token: String, done: (Result<JSONObject>) -> Unit) =
        run({ JSONObject(request("GET", "/api/smartpoule/phone/sheet?${identity()}&token=${enc(token)}", null)) }, done)

    /** Envoie le score d'un match : `{ status: ok|pending, id }`. */
    fun score(token: String, bout: Int, a: String, b: String, officiating: JSONObject? = null, done: (Result<JSONObject>) -> Unit) {
        val body = JSONObject()
            .put("c", pairing.club)
            .put("r", pairing.referee)
            .put("k", pairing.key)
            .put("token", token)
            .put("bout", bout)
            .put("values", JSONArray().put(a).put(b))
            .put("officiating", officiating)
            .toString()
        run({ JSONObject(request("POST", "/api/smartpoule/phone/score", body)) }, done)
    }

    fun officiating(token: String, bout: Int, state: JSONObject, done: (Result<JSONObject>) -> Unit) {
        val body = JSONObject().put("c", pairing.club).put("r", pairing.referee).put("k", pairing.key)
            .put("token", token).put("bout", bout).put("officiating", state).toString()
        run({ JSONObject(request("POST", "/api/smartpoule/phone/score", body)) }, done)
    }

    /** Rencontre par équipes : journal des relais, et le score final quand la rencontre est finie. */
    fun relay(token: String, bout: Int, log: JSONObject, values: List<String>?, done: (Result<JSONObject>) -> Unit) {
        val body = JSONObject().put("c", pairing.club).put("r", pairing.referee).put("k", pairing.key)
            .put("token", token).put("bout", bout).put("relay", log)
        values?.let { body.put("values", JSONArray(it)) }
        run({ JSONObject(request("POST", "/api/smartpoule/phone/score", body.toString())) }, done)
    }

    /** État en direct du match (score, chrono) pour l'écran LivePoule de la piste. */
    fun live(state: JSONObject, done: (Result<JSONObject>) -> Unit) {
        state.put("c", pairing.club).put("r", pairing.referee).put("k", pairing.key)
        run({ JSONObject(request("POST", "/api/smartpoule/phone/live", state.toString())) }, done)
    }

    /** Sort d'un score en attente : `{ status: pending|ok|error, error }`. */
    fun result(id: Long, done: (Result<JSONObject>) -> Unit) =
        run({ JSONObject(request("GET", "/api/smartpoule/phone/result?${identity()}&id=$id", null)) }, done)
}
