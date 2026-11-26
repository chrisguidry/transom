package sh.transom.signin

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.LaunchedApp
import sh.transom.testserver.ENDED
import sh.transom.testserver.TestServer
import sh.transom.theme.TransomTheme

/**
 * The app on screen goes to the sign-in screen once the server ends its session, says why, and
 * offers the same server again.
 */
@RunWith(RobolectricTestRunner::class)
class EndedSessionScreenTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()

    @Test
    fun theSignInScreenSaysTheServerSignedTheAppOut() {
        launched.signedIn(Session(server.url, ENDED, "bob"))
        compose.awaitText("transom signed you out. Sign in again to get your notifications.")
    }

    @Test
    fun itOffersTheSameServer() {
        launched.signedIn(Session(server.url, ENDED, "bob"))
        compose.awaitMain {
            compose
                .onAllNodes(hasSetTextAction() and hasText(server.url))
                .fetchSemanticsNodes()
                .isNotEmpty()
        }
    }
}

/** A session the person's sign-in provider ended says so on the sign-in screen. */
@RunWith(RobolectricTestRunner::class)
class EndedByIssuerTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun itSaysTheProviderEndedIt() {
        compose.setContent {
            TransomTheme { SignInScreen(Problem.Ended("issuer_ended"), false, onContinue = {}) }
        }
        compose
            .onNodeWithText(
                "Your sign-in provider ended your session. Sign in again to get your notifications.",
            )
            .assertExists()
    }
}
