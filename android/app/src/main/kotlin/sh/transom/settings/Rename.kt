package sh.transom.settings

import kotlinx.serialization.Serializable
import okhttp3.RequestBody.Companion.toRequestBody
import sh.transom.exchange.TransomJson
import sh.transom.sync.Client
import sh.transom.sync.JSON

/**
 * Renames [name] to [to], both without their `@`, and returns the name the server gave it. A name
 * someone else holds, or one that isn't valid, is refused with the server's reason.
 */
suspend fun Client.rename(name: String, to: String): String {
    val body = """{"name": ${TransomJson.encodeToString(to)}}""".toRequestBody(JSON)
    return TransomJson.decodeFromString<Named>(send(request(url("@$name")).patch(body))).name
}

@Serializable private class Named(val name: String)
