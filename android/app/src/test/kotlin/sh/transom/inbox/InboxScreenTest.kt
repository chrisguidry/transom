package sh.transom.inbox

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.LaunchedApp
import sh.transom.awaitOnline
import sh.transom.signin.awaitText
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.send

/** @bob's inbox, drawn from the phone's copy of a real server's. */
@RunWith(RobolectricTestRunner::class)
class InboxScreenTest {
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

    private fun ask(summary: String = "Deploy transom 1.4.2?"): String =
        server.send(alice, "bob", """{"summary": "$summary", "choices": [{"label": "Deploy"}]}""")

    @Test
    fun listsAQuestionUnderWaitingOnYou() {
        ask()
        launched.signedIn(server, "bob")
        compose.awaitText("Deploy transom 1.4.2?")
        assertTrue(top("WAITING ON YOU") < top("Deploy transom 1.4.2?"))
        assertTrue(top("Deploy transom 1.4.2?") < top("UNREAD"))
    }

    @Test
    fun listsWhatIsNotWaitingAndUnreadUnderUnread() {
        launched.signedIn(server, "bob")
        compose.awaitText("Hello")
        assertTrue(top("UNREAD") < top("Hello"))
    }

    @Test
    fun listsWhatIsReadUnderEverythingElse() {
        val id = server.send(alice, "bob", """{"summary": "Backup done"}""")
        server.call("PUT", "/exchanges/$id/@bob", bob)
        launched.signedIn(server, "bob")
        compose.awaitText("Backup done")
        assertTrue(top("EARLIER") < top("Backup done"))
    }

    @Test
    fun showsWhoSentItAndWhen() {
        ask()
        launched.signedIn(server, "bob")
        compose.awaitText("Deploy transom 1.4.2?")
        compose
            .onNode(hasText("Deploy transom 1.4.2?") and hasText("@alice") and hasText("now"))
            .assertIsDisplayed()
    }

    @Test
    fun marksWhatIsUnread() {
        ask()
        launched.signedIn(server, "bob")
        compose.awaitText("Deploy transom 1.4.2?")
        compose.onNode(hasText("Deploy transom 1.4.2?") and unread).assertIsDisplayed()
    }

    @Test
    fun showsWhatArrivesWhileItIsOnScreen() {
        launched.signedIn(server, "bob")
        compose.awaitText("Hello")
        ask("Restart nuc3?")
        compose.awaitText("Restart nuc3?")
    }

    @Test
    fun opensAnExchangeAndReadsIt() {
        val id = ask()
        launched.signedIn(server, "bob")
        compose.awaitText("Deploy transom 1.4.2?")
        compose.onNodeWithText("Deploy transom 1.4.2?").performClick()
        compose.awaitOnline {
            server.call("GET", "/exchanges/$id/@bob", bob) == """{"read":true}"""
        }
    }

    @Test
    fun saysNothingIsHereForSomeoneNew() {
        launched.signedIn(server, "carol")
        compose.awaitText("Nothing here yet.", substring = true)
        assertEquals(0, compose.onAllNodes(hasText("EARLIER")).fetchSemanticsNodes().size)
    }

    private fun top(text: String) =
        compose.onNodeWithText(text, substring = true).getBoundsInRoot().top
}

private val unread = SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, "Unread")
