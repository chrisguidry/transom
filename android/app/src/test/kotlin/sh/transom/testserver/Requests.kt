package sh.transom.testserver

import java.net.http.HttpRequest
import java.net.http.HttpResponse
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/**
 * Sends [method] [path] to the server as the holder of [token], with [body] as JSON, and returns
 * the answer's body, which must have the status [expected]. Tests act as the people around the app
 * with this, as curl or another client would, so the app's own client is tested only by what it
 * does itself.
 */
fun TestServer.call(
    method: String,
    path: String,
    token: String,
    body: String? = null,
    expected: Int = 200,
): String {
    val request =
        get("$url$path")
            .header("Authorization", "Bearer $token")
            .header("Content-Type", "application/json")
            .method(
                method,
                body?.let { HttpRequest.BodyPublishers.ofString(it) }
                    ?: HttpRequest.BodyPublishers.noBody(),
            )
    val response = http.send(request.build(), HttpResponse.BodyHandlers.ofString())
    check(response.statusCode() == expected) {
        "$method $path answered ${response.statusCode()}, not $expected:\n${response.body()}"
    }
    return response.body()
}

/**
 * Gives `@[name]`, the holder of [token], the display name [displayName] and the picture [picture],
 * the bytes of a square image, as whoever manages an identity sets them.
 */
fun TestServer.portray(token: String, name: String, displayName: String, picture: ByteArray) {
    call("PATCH", "/@$name", token, """{"display_name": "$displayName"}""")
    val request =
        get("$url/@$name/picture")
            .header("Authorization", "Bearer $token")
            .PUT(HttpRequest.BodyPublishers.ofByteArray(picture))
    val response = http.send(request.build(), HttpResponse.BodyHandlers.ofString())
    check(response.statusCode() in 200..299) {
        "PUT /@$name/picture answered ${response.statusCode()}:\n${response.body()}"
    }
}

/** Sends [message], a message as JSON, to `@[to]` as the holder of [token], and returns its id. */
fun TestServer.send(token: String, to: String, message: String): String =
    field(call("POST", "/@$to", token, message, expected = 201), "id")

/**
 * Makes the group `@[group]` with the holder of [founder]'s token as its first member, and has each
 * of [joining], a name and its token, accept an invitation to it.
 */
fun TestServer.group(group: String, founder: String, vararg joining: Pair<String, String>) {
    call("PUT", "/@$group", founder, expected = 201)
    for ((name, token) in joining) {
        val invited = call("PUT", "/@$group/members/@$name", founder, expected = 202)
        call("PUT", "${field(invited, "exchange")}/reply", token, """{"choice": "join"}""")
    }
}

/**
 * Has `@[recipient]`, the holder of [recipientToken], allow the holder of [senderToken] to send to
 * them, so what the sender sends next opens at once. The sender's first message is held for consent
 * and `@transom` asks the recipient about it, so the recipient's list keeps both: the first
 * message, a notification, and the request, replied to.
 */
fun TestServer.accept(recipient: String, recipientToken: String, senderToken: String) {
    send(senderToken, recipient, """{"summary": "Hello"}""")
    val listed = call("GET", "/@$recipient/exchanges?state=open&question=true", recipientToken)
    val request =
        Json.parseToJsonElement(listed).jsonObject.getValue("exchanges").jsonArray.single {
            it.jsonObject.getValue("from").jsonPrimitive.content == "@transom"
        }
    val id = request.jsonObject.getValue("id").jsonPrimitive.content
    call("PUT", "/exchanges/$id/reply", recipientToken, """{"choice": "allow"}""")
}

/** An address where no server listens, for a client that cannot reach its server. */
const val NOWHERE = "http://127.0.0.1:1"

/** The string at [name] in the JSON object [answer]. */
fun field(answer: String, name: String): String =
    checkNotNull(Json.parseToJsonElement(answer).jsonObject[name]) { "no $name in $answer" }
        .jsonPrimitive
        .content
