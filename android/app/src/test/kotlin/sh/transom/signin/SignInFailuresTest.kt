package sh.transom.signin

import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** The sign-in's calls at an address where nothing listens, or something other than transom. */
class SignInFailuresTest {
    @get:Rule val page = OtherServer()
    @get:Rule val missing = OtherServer(status = 404, body = "Not Found")

    private val requests = SignInRequests(OkHttpClient())

    @Test
    fun cannotReachAnAddressWhereNothingListens() {
        assertTrue(problemOf { requests.check(nowhere(), CHALLENGE) } is Problem.Unreachable)
    }

    @Test
    fun aWebPageIsNotTransom() {
        assertEquals(Problem.NotTransom, problemOf { requests.check(page.url, CHALLENGE) })
    }

    @Test
    fun aMissingPageIsNotTransom() {
        assertEquals(Problem.NotTransom, problemOf { requests.check(missing.url, CHALLENGE) })
    }

    @Test
    fun aWebPageRedeemsNoCode() {
        assertEquals(
            Problem.NotTransom,
            problemOf { requests.redeem(page.url, "code", "verifier") },
        )
    }

    @Test
    fun aMissingPageRedeemsNoCode() {
        assertEquals(
            Problem.NotTransom,
            problemOf { requests.redeem(missing.url, "code", "verifier") },
        )
    }

    @Test
    fun aWebPageNamesNoOne() {
        assertEquals(Problem.NotTransom, problemOf { requests.nameOf(page.url, "transom_token") })
    }

    @Test
    fun signingOutWhereNothingListensStillSucceeds() {
        requests.signOut(Session(nowhere(), "transom_token", "alice"))
    }

    private companion object {
        const val CHALLENGE = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    }
}
