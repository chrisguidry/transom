package sh.transom.signin

import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isHeading
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.testserver.TestServer

/** Signing in from the server's URL to the inbox, against a real transom and test-issuer. */
@RunWith(RobolectricTestRunner::class)
class SignInFlowTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()

    @Test
    fun headsTheScreenSignIn() {
        launch()
        compose.onNodeWithText("Sign in").assert(isHeading())
    }

    @Test
    fun opensTheServersSignInPageInACustomTab() {
        launch()
        compose.continueTo(server.url)
        val tab = compose.customTab()
        assertTrue(tab.hasExtra(CustomTabsIntent.EXTRA_SESSION))
        assertEquals("${server.url}/login", tab.data.toString().substringBefore('?'))
    }

    @Test
    fun asksTheServerToSendTheCodeToTheAppLink() {
        launch()
        compose.continueTo(server.url)
        assertEquals(APP_CALLBACK, compose.customTab().data!!.getQueryParameter("callback"))
    }

    @Test
    fun signsInToTheInboxOfTheNameTheServerGave() {
        val activity = launch()
        compose.continueTo(server.url)
        activity.newIntent(callback(server.signInApp("alice", compose.challengeSent())))
        compose.awaitText("@alice")
        compose.onAllNodesWithText("@alice").assertCountEquals(2)
    }

    @Test
    fun headsTheInboxWithTheName() {
        val activity = launch()
        compose.continueTo(server.url)
        activity.newIntent(callback(server.signInApp("alice", compose.challengeSent())))
        compose.awaitText("Nothing here yet.")
        compose.onNode(hasText("@alice") and isHeading()).assertExists()
    }

    @Test
    fun finishesASignInAfterAndroidStoppedTheApp() {
        val first = launch()
        compose.continueTo(server.url)
        val code = server.signInApp("alice", compose.challengeSent())
        first.pause().stop().destroy()
        launch(callback(code))
        compose.awaitText("Nothing here yet.")
    }

    @Test
    fun staysSignedInWhenTheAppStartsAgain() {
        val first = launch()
        compose.continueTo(server.url)
        first.newIntent(callback(server.signInApp("alice", compose.challengeSent())))
        compose.awaitText("Nothing here yet.")
        first.pause().stop().destroy()
        launch()
        compose.awaitText("Nothing here yet.")
    }
}
