package sh.transom.testserver

import java.net.URI
import java.net.http.HttpRequest
import java.net.http.HttpResponse
import java.security.MessageDigest
import java.util.Base64
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** The fixture runs a real server: a person can send to it, list from it, and sign the app in. */
class TestServerTest {
    @get:Rule val server = TestServer()

    @Test
    fun listsTheQuestionAliceSentHerself() {
        val alice = server.tokenFor("alice")
        val sent = send(post("/@alice", alice, "Is the fixture working?", "text/plain"))
        assertEquals(201, sent.statusCode())
        val listed = send(get("/@alice/exchanges", alice))
        assertTrue(listed.body(), listed.body().contains("Is the fixture working?"))
    }

    @Test
    fun theAppRedeemsItsSignInCodeForASession() {
        val verifier = "a-verifier-of-at-least-forty-three-characters-long"
        val code = server.signInApp("bob", challenge(verifier))
        val redeemed =
            send(
                post(
                    "/login/app",
                    token = null,
                    body = """{"code": "$code", "verifier": "$verifier"}""",
                    type = "application/json",
                )
            )
        val token = sessionToken.find(redeemed.body())!!.groupValues[1]
        val bob = send(get("/@bob", token))
        assertTrue(bob.body(), bob.body().contains(""""name":"bob""""))
    }

    private fun get(path: String, token: String): HttpRequest =
        HttpRequest.newBuilder(URI.create("${server.url}$path"))
            .header("Authorization", "Bearer $token")
            .build()

    private fun post(path: String, token: String?, body: String, type: String): HttpRequest {
        val request =
            HttpRequest.newBuilder(URI.create("${server.url}$path"))
                .header("Content-Type", type)
                .POST(HttpRequest.BodyPublishers.ofString(body))
        token?.let { request.header("Authorization", "Bearer $it") }
        return request.build()
    }

    private fun send(request: HttpRequest): HttpResponse<String> =
        server.http.send(request, HttpResponse.BodyHandlers.ofString())
}

private val sessionToken = Regex(""""token"\s*:\s*"(transom_[^"]+)"""")

/** The S256 challenge of a PKCE verifier, as RFC 7636 defines it. */
private fun challenge(verifier: String): String =
    Base64.getUrlEncoder()
        .withoutPadding()
        .encodeToString(MessageDigest.getInstance("SHA-256").digest(verifier.toByteArray()))
