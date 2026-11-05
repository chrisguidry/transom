package sh.transom.signin

import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import sh.transom.testserver.TestServer

/** Each problem a person can meet while signing in gets a plain message on the sign-in screen. */
@RunWith(RobolectricTestRunner::class)
class SignInProblemsTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val other = OtherServer()

    @Test
    fun asksForAnAddressWhenGivenNone() {
        launch()
        compose.continueTo(" ")
        compose.awaitText("Enter your transom server's address, such as transom.example.com.")
    }

    @Test
    fun saysWhenTheServerCannotBeReached() {
        launch()
        compose.continueTo(nowhere())
        compose.awaitText(
            "That server can't be reached. Check the address and your connection, then try again."
        )
    }

    @Test
    fun saysWhenTheAddressIsNotTransom() {
        launch()
        compose.continueTo(other.url)
        compose.awaitText("That address isn't a transom server. Check it and try again.")
    }

    @Test
    fun saysWhenNoBrowserCanSignIn() {
        shadowOf(app).checkActivities(true)
        launch()
        compose.continueTo(server.url)
        compose.awaitText("Signing in needs a web browser. Install one, then try again.")
    }

    @Test
    fun saysWhenTheCustomTabClosedBeforeTheSignInFinished() {
        val activity = launch()
        compose.continueTo(server.url)
        compose.customTab()
        activity.pause().resume()
        compose.awaitText("Signing in was cancelled. Try again when you're ready.")
    }

    @Test
    fun saysWhenTheServerRefusedTheCode() {
        val activity = launch()
        compose.continueTo(server.url)
        compose.customTab()
        activity.newIntent(callback("a-code-the-server-never-gave"))
        compose.awaitText("The server didn't accept that sign-in. Try again.")
    }

    @Test
    fun saysWhenACallbackArrivesForNoSignIn() {
        launch(callback("a-code-from-long-ago"))
        compose.awaitText("The server didn't accept that sign-in. Try again.")
    }
}
