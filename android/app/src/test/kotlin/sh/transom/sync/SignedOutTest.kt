package sh.transom.sync

import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.runBlocking
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized

/**
 * A client whose every request is answered with [status], the challenge [challenge], and [body], as
 * transom or a proxy in front of it would answer, and which adds each code it hears of to [heard].
 */
private fun answering(
    challenge: String?,
    body: String = ENDED_BODY,
    heard: MutableList<String?> = mutableListOf(),
    status: Int = 401,
): Client {
    val http =
        OkHttpClient.Builder()
            .addInterceptor { chain ->
                Response.Builder()
                    .request(chain.request())
                    .protocol(Protocol.HTTP_1_1)
                    .code(status)
                    .message("Unauthorized")
                    .apply { challenge?.let { header("WWW-Authenticate", it) } }
                    .body(body.toResponseBody("application/json".toMediaType()))
                    .build()
            }
            .build()
    return Client("https://transom.example.com", "transom_token", http) { heard += it }
}

private const val TRANSOMS = """Bearer realm="transom", error="invalid_token""""

private const val ENDED_BODY = """{"error": "the session has ended", "code": "session_ended"}"""

/** transom's own 401 says the session is over, whichever call hears it. */
class SignedOutTest {
    @Test
    fun transomsOwn401SaysTheSessionIsOver() {
        val ended =
            assertThrows(SignedOut::class.java) {
                runBlocking { answering(TRANSOMS).exchanges("bob") }
            }
        assertEquals("session_ended", ended.code)
    }

    @Test
    fun theClientHearsOfItOnce() {
        val heard = mutableListOf<String?>()
        assertThrows(SignedOut::class.java) {
            runBlocking { answering(TRANSOMS, heard = heard).exchanges("bob") }
        }
        assertEquals(listOf<String?>("session_ended"), heard)
    }

    @Test
    fun aBodyWithNoCodeHasNone() {
        val ended =
            assertThrows(SignedOut::class.java) {
                runBlocking { answering(TRANSOMS, body = """{"error": "no"}""").picture("/p") }
            }
        assertEquals(null, ended.code)
    }

    @Test
    fun theLiveFeedHearsItToo() {
        val heard = mutableListOf<String?>()
        assertThrows(SignedOut::class.java) {
            runBlocking { answering(TRANSOMS, heard = heard).feed("bob").collect() }
        }
        assertEquals(listOf<String?>("session_ended"), heard)
    }

    @Test
    fun anotherFailureIsARefusal() {
        val refused =
            assertThrows(Refused::class.java) {
                runBlocking { answering(TRANSOMS, status = 403).exchanges("bob") }
            }
        assertEquals(403, refused.status)
    }
}

/** A 401 without transom's challenge came from something in front of it, and is worth retrying. */
@RunWith(Parameterized::class)
class ProxysUnauthorizedTest(private val challenge: String?) {
    @Test
    fun itIsATransientRefusal() {
        val heard = mutableListOf<String?>()
        val refused =
            assertThrows(Refused::class.java) {
                runBlocking { answering(challenge, heard = heard).exchanges("bob") }
            }
        assertTrue(refused.transient && heard.isEmpty())
    }

    companion object {
        @JvmStatic
        @Parameterized.Parameters(name = "{0}")
        fun cases() =
            listOf(
                arrayOf<Any?>(null),
                arrayOf<Any?>("""Basic realm="proxy""""),
                arrayOf<Any?>("""Bearer realm="proxy""""),
            )
    }
}
