package sh.transom.groups

import kotlinx.serialization.Serializable
import okhttp3.RequestBody.Companion.toRequestBody
import sh.transom.exchange.TransomJson
import sh.transom.sync.Client

/** The members of [group], a name without its `@`, each a name with its `@`, in order. */
suspend fun Client.members(group: String): List<String> =
    TransomJson.decodeFromString<Listing>(send(request(url("@$group", "members")))).members

/**
 * Invites [name] to [group], both without their `@`. `@transom` asks them first, and they join when
 * they accept. Returns `false` when they are a member already, and nothing was asked.
 */
suspend fun Client.invite(group: String, name: String): Boolean {
    val answer =
        send(request(url("@$group", "members", "@$name")).put(ByteArray(0).toRequestBody()))
    return TransomJson.decodeFromString<Invited>(answer).invited != null
}

/** Removes [name] from [group], both without their `@`. Removing yourself is leaving. */
suspend fun Client.remove(group: String, name: String) {
    send(request(url("@$group", "members", "@$name")).delete())
}

@Serializable private class Listing(val members: List<String>)

/** The server's answer to an invitation: whom it invited, or `null` for a member already. */
@Serializable private class Invited(val invited: String? = null)
