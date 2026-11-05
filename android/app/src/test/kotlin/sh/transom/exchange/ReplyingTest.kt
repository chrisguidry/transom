package sh.transom.exchange

import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isHeading
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.LaunchedApp
import sh.transom.awaitOnline
import sh.transom.signin.awaitMain
import sh.transom.signin.awaitText
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.field
import sh.transom.testserver.group
import sh.transom.testserver.send

/** @bob answers @alice's question from its screen, through the outbox, on a real server. */
@RunWith(RobolectricTestRunner::class)
class ReplyingTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()

    private lateinit var alice: String
    private lateinit var bob: String

    @Before
    fun people() {
        alice = server.tokenFor("alice")
        bob = server.tokenFor("bob")
    }

    /** Bob opens [summary] from his inbox. */
    private fun open(summary: String) {
        launched.signedIn(server, "bob")
        compose.awaitText(summary)
        compose.onNodeWithText(summary).performClick()
        compose.awaitMain {
            compose.onAllNodes(hasText(summary) and isHeading()).fetchSemanticsNodes().isNotEmpty()
        }
    }

    /** Alice asks Bob whether to deploy, and Bob opens it. */
    private fun asked(): String {
        server.accept("bob", bob, alice)
        val id = server.send(alice, "bob", message("Deploy?", choices = DEPLOY_OR_HOLD))
        open("Deploy?")
        return id
    }

    private fun choose(label: String) {
        compose.onNode(hasText(label) and hasClickAction()).performClick()
    }

    /**
     * The [part] of exchange [id]'s reply on the server, such as its choice, or null before one.
     */
    private fun replied(id: String, part: String): String? = runCatching {
        field(server.call("GET", "/exchanges/$id/reply", alice), part)
    }
        .getOrNull()

    @Test
    fun aChoiceShowsAsSendingAtOnce() {
        asked()
        choose("Deploy")
        compose.awaitText("@bob replied · sending")
    }

    @Test
    fun aChoiceIsSentOnceThePhoneIsOnline() {
        val id = asked()
        choose("Deploy")
        compose.awaitOnline { replied(id, "choice") == "Deploy" }
    }

    @Test
    fun aSentReplySettlesAsTheServersReply() {
        asked()
        choose("Deploy")
        compose.awaitOnline {
            compose
                .onAllNodes(hasText("· replied", substring = true))
                .fetchSemanticsNodes()
                .isNotEmpty()
        }
        compose.onNodeWithText("Deploy").assertExists()
    }

    @Test
    fun aNoteGoesWithTheChoice() {
        val id = asked()
        compose
            .onNode(hasContentDescription("Add a note, if you like"))
            .performTextInput("after lunch ")
        choose("Hold")
        compose.awaitOnline { replied(id, "text") == "after lunch" }
    }

    @Test
    fun aQuestionWithOnlyAPromptIsAnsweredWithText() {
        server.accept("bob", bob, alice)
        val id = server.send(alice, "bob", message("Why?", prompt = "Say why it failed"))
        open("Why?")
        compose
            .onNode(hasContentDescription("Say why it failed"))
            .performTextInput("The disk filled")
        compose.onNode(hasText("Reply") and hasClickAction()).performClick()
        compose.awaitOnline { replied(id, "text") == "The disk filled" }
    }

    @Test
    fun replyWaitsForSomethingToSay() {
        server.accept("bob", bob, alice)
        server.send(alice, "bob", message("Why?", prompt = "Say why it failed"))
        open("Why?")
        compose.onNode(hasText("Reply") and hasClickAction()).assertIsNotEnabled()
    }

    @Test
    fun aReplyThatCameFirstIsShownInPlaceOfBobs() {
        server.group("team", alice, "bob" to bob)
        val id = server.send(alice, "team", message("Deploy?", choices = DEPLOY_OR_HOLD))
        open("Deploy?")
        choose("Deploy")
        compose.awaitText("@bob replied · sending")
        server.call("PUT", "/exchanges/$id/reply", alice, """{"choice": "Hold"}""")
        compose.awaitOnline {
            compose
                .onAllNodes(hasText("@alice replied", substring = true))
                .fetchSemanticsNodes()
                .isNotEmpty()
        }
        assertEquals("Hold", replied(id, "choice"))
        compose.onNodeWithText("Hold").assertExists()
    }

    private companion object {
        val DEPLOY_OR_HOLD = listOf("Deploy" to null, "Hold" to null)
    }
}
