package sh.transom.sync

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonObject
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.coroutines.executeAsync
import sh.transom.exchange.Exchange
import sh.transom.exchange.Reply
import sh.transom.exchange.State
import sh.transom.exchange.TransomJson

/**
 * transom's HTTP API as one person, with a bearer token: an issuer's access token, or the session
 * token the app gets when it signs in.
 *
 * Each call throws [java.io.IOException] when the server cannot be reached, and [Refused] when it
 * answers with a failure.
 */
class Client(
    server: String,
    private val token: String,
    internal val http: OkHttpClient = OkHttpClient(),
) {
    private val base: HttpUrl = server.trimEnd('/').toHttpUrl()

    /** [name]'s exchanges, sent and received, newest first, that pass [filters]. */
    suspend fun exchanges(name: String, filters: Filters = Filters()): List<Exchange> {
        val url = url("@$name", "exchanges").newBuilder()
        filters.state?.let { url.addQueryParameter("state", TransomJson.encodeState(it)) }
        filters.read?.let { url.addQueryParameter("read", it.toString()) }
        filters.question?.let { url.addQueryParameter("question", it.toString()) }
        filters.limit?.let { url.addQueryParameter("limit", it.toString()) }
        return TransomJson.decodeFromString<Listing>(send(request(url.build()))).exchanges
    }

    /** The whole exchange [id], as this person sees it. */
    suspend fun exchange(id: String): Exchange =
        TransomJson.decodeFromString(send(request(url("exchanges", id))))

    /**
     * Replies to exchange [id] with [choice], a choice's key or label, and [text], at least one of
     * them. Repeating the same reply is safe, so a reply queued offline can be sent again.
     */
    suspend fun reply(id: String, choice: String? = null, text: String? = null): Replied {
        val body = TransomJson.encodeToString(ReplyBody(choice, text)).toRequestBody(JSON)
        val request = request(url("exchanges", id, "reply")).put(body)
        return try {
            Replied.Accepted(TransomJson.decodeFromString(send(request)))
        } catch (refused: Refused) {
            Replied.Lost(refused.winner ?: throw refused)
        }
    }

    /** Takes back the message of exchange [id], which only its sender may do while it is open. */
    suspend fun withdraw(id: String): Exchange =
        TransomJson.decodeFromString(send(request(url("exchanges", id, "message")).delete()))

    /** Marks exchange [id] [read] or unread for [name], this person, and returns the new state. */
    suspend fun mark(id: String, name: String, read: Boolean): Boolean {
        val url = request(url("exchanges", id, "@$name"))
        val request = if (read) url.put(ByteArray(0).toRequestBody()) else url.delete()
        return TransomJson.decodeFromString<ReadState>(send(request)).read
    }

    /**
     * Registers this phone as [name]'s device [device], called [deviceName], which wakes by holding
     * the live feed open, so the server pushes nothing to it. Registering again replaces it.
     */
    suspend fun register(name: String, device: String, deviceName: String) {
        val body = buildJsonObject {
            put("name", deviceName)
            putJsonObject("wake") { put("connection", true) }
        }
        val request =
            request(url("@$name", "devices", device)).put(body.toString().toRequestBody(JSON))
        send(request)
    }

    internal fun url(vararg segments: String): HttpUrl =
        base.newBuilder().apply { segments.forEach { addPathSegment(it) } }.build()

    internal fun request(url: HttpUrl): Request.Builder =
        Request.Builder().url(url).header("Authorization", "Bearer $token")

    /** Sends [request] and returns the body of a success, or throws [Refused] with the failure. */
    private suspend fun send(request: Request.Builder): String =
        withContext(Dispatchers.IO) {
            http.newCall(request.header("Accept", "application/json").build()).executeAsync().use {
                val body = it.body.string()
                if (!it.isSuccessful) throw Refused.of(it.code, body)
                body
            }
        }
}

private val JSON = "application/json".toMediaType()

/** What the list at `/@{name}/exchanges` can be narrowed to. Each filter left `null` is not one. */
data class Filters(
    val state: State? = null,
    val read: Boolean? = null,
    val question: Boolean? = null,
    /** How many to list: the server lists 50 unless asked, and [MOST] at most. */
    val limit: Int? = null,
) {
    companion object {
        /** The most exchanges the server lists at once. */
        const val MOST = 200
    }
}

/** How a reply turned out. */
sealed interface Replied {
    /** The reply is the exchange's reply, and this is the exchange with it. */
    data class Accepted(val exchange: Exchange) : Replied

    /** Someone else replied first, with [winner]: the first reply wins. */
    data class Lost(val winner: Reply) : Replied
}

/**
 * The server answered [status] with [reason]. [winner] is the reply that won when a reply came
 * after another one.
 */
class Refused(val status: Int, val reason: String, val winner: Reply? = null) :
    Exception("transom answered $status: $reason") {
    /**
     * Whether the same request may succeed later: the server or something before it is failing for
     * now, or the session has to be signed in again. Any other failure would fail again.
     */
    val transient: Boolean
        get() = status == 401 || status == 408 || status == 429 || status >= 500

    internal companion object {
        /**
         * The failure in [body], which transom writes as `{"error": …}` and anything else may not.
         */
        fun of(status: Int, body: String): Refused {
            val failure = runCatching { TransomJson.decodeFromString<Failure>(body) }.getOrNull()
            return Refused(status, failure?.error ?: body, failure?.reply)
        }
    }
}

@Serializable private class Failure(val error: String, val reply: Reply? = null)

@Serializable private class Listing(val exchanges: List<Exchange>)

@Serializable private class ReplyBody(val choice: String?, val text: String?)

@Serializable private class ReadState(val read: Boolean)

/** A state as the server spells it, such as `open`. */
private fun Json.encodeState(state: State): String =
    (encodeToJsonElement(State.serializer(), state) as JsonPrimitive).content
