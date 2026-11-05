package sh.transom

import androidx.test.platform.app.InstrumentationRegistry
import java.net.HttpURLConnection
import java.net.URI
import java.net.URLEncoder
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/**
 * The transom and `test-issuer` that `end-to-end.sh` runs on the machine beside the emulator, and
 * that `adb reverse` brings to the same port of the phone's `127.0.0.1`. The script names both URLs
 * in the instrumentation's arguments `server` and `issuer`.
 *
 * The test acts as the people around the app with these requests, as curl would, so only the app
 * itself takes the path through the phone's screen.
 */
class HostServer {
    private val arguments = InstrumentationRegistry.getArguments()

    /** transom's base URL, such as `http://127.0.0.1:41623`, which the person types to sign in. */
    val url: String = argument("server")

    private val issuer = argument("issuer")

    private fun argument(name: String): String =
        checkNotNull(arguments.getString(name)) {
            "the instrumentation has no argument $name: run the test with end-to-end.sh"
        }

    /**
     * An access token from the issuer for `@[name]`, whose subject is also [name], after a request
     * with it that creates `@name` on transom, so others can send to it at once.
     */
    fun tokenFor(name: String): String {
        val token =
            request("GET", "$issuer/access-token?subject=${encoded(name)}&name=${encoded(name)}")
        call("GET", "/@$name", token)
        return token
    }

    /**
     * Sends [method] [path] to transom as the holder of [token], with [body] as JSON, and returns
     * the answer's body, which must have the status [expected].
     */
    fun call(
        method: String,
        path: String,
        token: String,
        body: String? = null,
        expected: Int = 200,
    ): String = request(method, "$url$path", token, body, expected)

    /** Sends [message], as JSON, to `@[to]` as the holder of [token], and returns its id. */
    fun send(token: String, to: String, message: String): String =
        field(call("POST", "/@$to", token, message, expected = 201), "id")

    /**
     * Has `@[recipient]`, the holder of [recipientToken], allow the holder of [senderToken] to send
     * to them. The sender's first message is held for consent, and `@transom` asks the recipient
     * about it, so this sends one and answers that question.
     */
    fun accept(recipient: String, recipientToken: String, senderToken: String) {
        send(senderToken, recipient, """{"summary": "Hello"}""")
        val listed = call("GET", "/@$recipient/exchanges?state=open&question=true", recipientToken)
        val request =
            Json.parseToJsonElement(listed).jsonObject.getValue("exchanges").jsonArray.single {
                it.jsonObject.getValue("from").jsonPrimitive.content == "@transom"
            }
        val id = request.jsonObject.getValue("id").jsonPrimitive.content
        call("PUT", "/exchanges/$id/reply", recipientToken, """{"choice": "allow"}""")
    }

    /** Exchange [id] as the holder of [token] reads it from transom. */
    fun exchange(id: String, token: String): JsonObject =
        Json.parseToJsonElement(call("GET", "/exchanges/$id", token)).jsonObject

    private fun request(
        method: String,
        url: String,
        token: String? = null,
        body: String? = null,
        expected: Int = 200,
    ): String {
        val connection = URI.create(url).toURL().openConnection() as HttpURLConnection
        try {
            connection.requestMethod = method
            connection.connectTimeout = TIMEOUT_MS
            connection.readTimeout = TIMEOUT_MS
            token?.let { connection.setRequestProperty("Authorization", "Bearer $it") }
            body?.let {
                connection.doOutput = true
                connection.setRequestProperty("Content-Type", "application/json")
                connection.outputStream.use { stream -> stream.write(it.toByteArray()) }
            }
            val status = connection.responseCode
            val answer =
                (if (status < 400) connection.inputStream else connection.errorStream)
                    ?.bufferedReader()
                    ?.use { it.readText() }
                    .orEmpty()
            check(status == expected) { "$method $url answered $status, not $expected:\n$answer" }
            return answer
        } finally {
            connection.disconnect()
        }
    }

    private companion object {
        const val TIMEOUT_MS = 10_000

        fun encoded(value: String): String = URLEncoder.encode(value, "UTF-8")

        fun field(answer: String, name: String): String =
            checkNotNull(Json.parseToJsonElement(answer).jsonObject[name]) { "no $name in $answer" }
                .jsonPrimitive
                .content
    }
}
