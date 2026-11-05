package sh.transom.signin

import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import sh.transom.testserver.TestServer

/** The sign-in's calls against a real transom and test-issuer. */
class SignInRequestsTest {
    @get:Rule val server = TestServer()

    private val requests = SignInRequests(OkHttpClient())
    private val verifier = newVerifier()

    @Test
    fun findsTransomAtItsUrl() {
        requests.check(server.url, challengeOf(verifier))
    }

    @Test
    fun redeemsTheCodeForASessionToken() {
        assertTrue(redeemed("alice").startsWith("transom_"))
    }

    @Test
    fun learnsTheNameTheSessionIsFor() {
        assertEquals("alice", requests.nameOf(server.url, redeemed("alice")))
    }

    @Test
    fun signingOutEndsTheSession() {
        val token = redeemed("alice")
        requests.signOut(Session(server.url, token, "alice"))
        assertEquals(Problem.Refused, problemOf { requests.nameOf(server.url, token) })
    }

    @Test
    fun refusesACodeWithAnotherVerifier() {
        val code = server.signInApp("alice", challengeOf(verifier))
        assertEquals(
            Problem.Refused,
            problemOf { requests.redeem(server.url, code, newVerifier()) },
        )
    }

    @Test
    fun refusesACodeSpentAlready() {
        val code = server.signInApp("alice", challengeOf(verifier))
        requests.redeem(server.url, code, verifier)
        assertEquals(Problem.Refused, problemOf { requests.redeem(server.url, code, verifier) })
    }

    private fun redeemed(name: String): String =
        requests.redeem(server.url, server.signInApp(name, challengeOf(verifier)), verifier)
}

internal fun problemOf(call: () -> Unit): Problem =
    assertThrows(SignInFailed::class.java) { call() }.problem
