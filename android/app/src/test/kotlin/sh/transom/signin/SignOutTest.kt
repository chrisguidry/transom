package sh.transom.signin

import androidx.compose.ui.test.assert
import androidx.compose.ui.test.isHeading
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import okhttp3.OkHttpClient
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.testserver.TestServer

/** Signing out from the top bar's menu, against a real transom and test-issuer. */
@RunWith(RobolectricTestRunner::class)
class SignOutTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()

    private val requests = SignInRequests(OkHttpClient())

    @Test
    fun returnsToTheSignInScreen() {
        app.sessions.keep(Session(server.url, signedIn("alice"), "alice"))
        launch()
        signOut()
        compose.onNodeWithText("Sign in").assert(isHeading())
    }

    @Test
    fun endsTheSessionOnTheServer() {
        val token = signedIn("alice")
        app.sessions.keep(Session(server.url, token, "alice"))
        launch()
        signOut()
        compose.waitUntil(10_000) {
            runCatching { requests.nameOf(server.url, token) }
                .exceptionOrNull()
                .let { it is SignInFailed && it.problem == Problem.Refused }
        }
    }

    @Test
    fun forgetsTheSessionWhenTheServerCannotBeReached() {
        app.sessions.keep(Session(nowhere(), "transom_token", "alice"))
        launch()
        signOut()
        assertNull(app.sessions.session())
    }

    private fun signOut() {
        compose.onNodeWithContentDescription("Menu").performClick()
        compose.onNodeWithText("Sign out").performClick()
    }

    /** A real session's token for [name], from the sign-in's own calls. */
    private fun signedIn(name: String): String {
        val verifier = newVerifier()
        return requests.redeem(server.url, server.signInApp(name, challengeOf(verifier)), verifier)
    }
}
