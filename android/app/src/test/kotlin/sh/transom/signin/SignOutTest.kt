package sh.transom.signin

import android.app.NotificationManager
import android.content.Context
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
import org.robolectric.Shadows.shadowOf
import sh.transom.testserver.TestServer
import sh.transom.testserver.call
import sh.transom.testserver.sessionFor

/** Signing out from the top bar's menu, against a real transom and test-issuer. */
@RunWith(RobolectricTestRunner::class)
class SignOutTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()

    private val requests = SignInRequests(OkHttpClient())

    @Test
    fun returnsToTheSignInScreen() {
        app.sessions.keep(Session(server.url, server.sessionFor("alice"), "alice"))
        launch()
        signOut()
        compose.onNodeWithText("Sign in").assert(isHeading())
    }

    @Test
    fun endsTheSessionOnTheServer() {
        val token = server.sessionFor("alice")
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

    @Test
    fun forgetsThePhoneAsADeviceFirst() {
        // The issuer's token names @alice, so the server knows her before the phone registers.
        val alice = server.tokenFor("alice")
        val devices = { server.call("GET", "/@alice/devices", alice) }
        app.sessions.keep(Session(server.url, server.sessionFor("alice"), "alice"))
        launch()
        compose.awaitMain { devices().contains(""""connection":true""") }
        signOut()
        compose.awaitMain { !devices().contains(""""connection":true""") }
    }

    @Test
    fun postsNoNotificationThatTheServerSignedTheAppOut() {
        app.sessions.keep(Session(server.url, server.sessionFor("alice"), "alice"))
        launch()
        signOut()
        compose.awaitMain { app.account() == null }
        assertNull(shadowOf(manager).getNotification(SignedOutNotification.NOTIFICATION))
    }

    private val manager: NotificationManager
        get() = app.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

    private fun signOut() {
        compose.onNodeWithContentDescription("Menu").performClick()
        compose.onNodeWithText("Sign out").performClick()
    }
}
