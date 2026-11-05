package sh.transom.exchange

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isHeading
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.LaunchedApp
import sh.transom.signin.awaitMain
import sh.transom.signin.awaitText
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.send

/** An exchange's own screen, as @bob opens it from his inbox on a real server. */
@RunWith(RobolectricTestRunner::class)
class ExchangeScreenTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()

    private lateinit var alice: String
    private lateinit var bob: String

    @Before
    fun people() {
        alice = server.tokenFor("alice")
        bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
    }

    /** Alice sends Bob [message], and Bob opens it from his inbox by its [summary]. */
    private fun opened(summary: String, message: String = message(summary)): String {
        val id = server.send(alice, "bob", message)
        launched.signedIn(server, "bob")
        compose.awaitText(summary)
        compose.onNodeWithText(summary).performClick()
        compose.awaitMain {
            compose.onAllNodes(hasText(summary) and isHeading()).fetchSemanticsNodes().isNotEmpty()
        }
        return id
    }

    @Test
    fun headsTheScreenWithTheSummary() {
        opened("Deploy transom 1.4.2?")
        compose.onNode(hasText("Deploy transom 1.4.2?") and isHeading()).assertIsDisplayed()
    }

    @Test
    fun rendersTheBody() {
        opened("Deploy?", message("Deploy?", body = "The build is **green**."))
        compose.onNodeWithText("The build is green.").assertIsDisplayed()
    }

    @Test
    fun offersEachChoiceAsAButton() {
        opened("Deploy?", message("Deploy?", choices = listOf("Deploy" to null, "Hold" to null)))
        compose.onNode(hasText("Deploy") and hasClickAction()).assertIsDisplayed()
        compose.onNode(hasText("Hold") and hasClickAction()).assertIsDisplayed()
    }

    @Test
    fun describesAChoiceAfterItsLabel() {
        val choices = listOf("Deploy" to "Roll out to **every** host.", "Hold" to null)
        opened("Deploy?", message("Deploy?", choices = choices))
        compose.onNodeWithText("Deploy Roll out to every host.").assertIsDisplayed()
    }

    @Test
    fun asksWithThePrompt() {
        opened("Why?", message("Why?", prompt = "Say why it failed"))
        compose.onNodeWithText("Say why it failed").assertIsDisplayed()
    }

    @Test
    fun asksForANoteWhenThereIsNoPrompt() {
        opened("Deploy?", message("Deploy?", choices = listOf("Deploy" to null)))
        compose.onNodeWithText("Add a note, if you like").assertIsDisplayed()
    }

    @Test
    fun listsTheMessagesLinks() {
        val links = listOf("https://ci.example.com/1" to "related", "https://example.com" to null)
        opened("Built", message("Built", links = links))
        compose.onNodeWithText("related: https://ci.example.com/1").assertIsDisplayed()
        compose.onNodeWithText("https://example.com").assertIsDisplayed()
    }

    @Test
    fun showsTheReplyOnceThereIsOne() {
        val id = server.send(alice, "bob", message("Deploy?", choices = listOf("Deploy" to null)))
        server.call("PUT", "/exchanges/$id/reply", bob, """{"choice": "Deploy", "text": "Done"}""")
        launched.signedIn(server, "bob")
        compose.awaitText("Deploy?")
        compose.onNodeWithText("Deploy?").performClick()
        compose.awaitText("Done")
        compose.onNodeWithText("Deploy").assertIsDisplayed()
    }

    @Test
    fun offersNoAnswerToAWithdrawnQuestion() {
        val id = server.send(alice, "bob", message("Deploy?", choices = listOf("Deploy" to null)))
        server.call("DELETE", "/exchanges/$id/message", alice)
        launched.signedIn(server, "bob")
        compose.awaitText("Deploy?")
        compose.onNodeWithText("Deploy?").performClick()
        compose.awaitText("withdrawn", substring = true)
        compose.onNodeWithText("Add a note, if you like").assertDoesNotExist()
    }
}
