package sh.transom.exchange

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.android.controller.ActivityController
import org.robolectric.annotation.GraphicsMode
import sh.transom.LaunchedApp
import sh.transom.MainActivity
import sh.transom.awaitOnline
import sh.transom.signin.app
import sh.transom.signin.awaitHeading
import sh.transom.signin.awaitText
import sh.transom.sync.Sending
import sh.transom.testserver.TestServer
import sh.transom.testserver.call
import sh.transom.testserver.group
import sh.transom.testserver.send

/**
 * @bob answers a question to his team after Alice has, and the app says that his answer was not
 *   used, until he has seen that.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LostReplyTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()

    private lateinit var alice: String
    private lateinit var activity: ActivityController<MainActivity>
    private lateinit var id: String

    /** Alice, whose display name is Alice, asks the team whether to deploy, and Bob's app opens. */
    @Before
    fun asked() {
        alice = server.tokenFor("alice")
        server.call("PATCH", "/@alice", alice, """{"display_name": "Alice"}""")
        server.group("team", alice, "bob" to server.tokenFor("bob"))
        id = server.send(alice, "team", message("Deploy?", choices = DEPLOY_OR_HOLD))
        activity = launched.signedIn(server, "bob")
        compose.awaitText("Deploy?")
    }

    private fun shows(text: String) =
        compose.onAllNodes(hasText(text, substring = true)).fetchSemanticsNodes().isNotEmpty()

    /**
     * Bob answers Deploy with the phone offline, Alice answers Hold, and the phone comes online.
     */
    private fun bobAnswersAfterAlice() {
        compose.onNode(hasText("Deploy") and hasClickAction()).performClick()
        compose.awaitText("Your answer, Deploy, is not sent yet", substring = true)
        server.call("PUT", "/exchanges/$id/reply", alice, """{"choice": "Hold"}""")
        compose.awaitOnline { shows(NOT_USED) }
    }

    /** Bob opens the question, and answers it there after Alice has. */
    private fun bobAnswersOnItsScreenAfterAlice() {
        compose.onNodeWithText("Deploy?").performClick()
        compose.awaitHeading("Deploy?")
        compose.onNode(hasText("Deploy") and hasClickAction()).performClick()
        compose.awaitText("NOT SENT YET")
        server.call("PUT", "/exchanges/$id/reply", alice, """{"choice": "Hold"}""")
        compose.awaitOnline { shows("NOT USED") }
    }

    private fun top(text: String) =
        compose.onNode(hasText(text, substring = true)).fetchSemanticsNode().boundsInRoot.top

    /** What the phone keeps of Bob's answer that was not used. */
    private fun kept(): Sending? = runBlocking {
        checkNotNull(app.account()).store.exchange(id).first()?.lost
    }

    @Test
    fun theRowSaysTheAnswerWasNotUsedAndWhoAnsweredFirst() {
        bobAnswersAfterAlice()
        compose.onNodeWithText("Alice · to @team · $NOT_USED").assertExists()
    }

    @Test
    fun markingTheRowReadForgetsIt() {
        bobAnswersAfterAlice()
        compose.onNode(hasText("Deploy?")).performTouchInput { swipeLeft() }
        compose.awaitText("Alice · to @team · Alice answered: Hold")
    }

    @Test
    fun theScreenSaysTheAnswerWasNotUsedOverTheOneThatWas() {
        bobAnswersOnItsScreenAfterAlice()
        compose.onNodeWithText(NOT_USED).assertExists()
        assertEquals(true, top("NOT USED") < top("Alice replied"))
    }

    @Test
    fun leavingTheScreenForgetsIt() {
        bobAnswersOnItsScreenAfterAlice()
        activity.get().onBackPressedDispatcher.onBackPressed()
        compose.awaitText("Alice · to @team · Alice answered: Hold")
    }

    @Test
    fun turningThePhoneKeepsIt() {
        bobAnswersOnItsScreenAfterAlice()
        activity.recreate()
        compose.awaitText("NOT USED")
        assertEquals(Sending("Deploy", null), kept())
    }

    private companion object {
        val DEPLOY_OR_HOLD = listOf("Deploy" to null, "Hold" to null)
        const val NOT_USED = "Your answer, Deploy, was not used: Alice answered first"
    }
}
