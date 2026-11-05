package sh.transom.exchange

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
import org.robolectric.annotation.GraphicsMode
import sh.transom.LaunchedApp
import sh.transom.signin.awaitMain
import sh.transom.signin.awaitText
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.portray
import sh.transom.testserver.send

/** Who an exchange is between, and its links, by the names their owners gave them. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ExchangePartiesTest {
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
        server.portray(alice, "alice", "Alertmanager", png(8, 8))
    }

    private fun opened(summary: String) {
        launched.signedIn(server, "bob")
        compose.awaitText(summary)
        compose.onNodeWithText(summary).performClick()
        compose.awaitMain {
            compose.onAllNodes(hasText(summary) and isHeading()).fetchSemanticsNodes().isNotEmpty()
        }
    }

    @Test
    fun namesTheSenderByItsDisplayNameBesideItsHandle() {
        server.send(alice, "bob", """{"summary": "Postgres is down"}""")
        opened("Postgres is down")
        compose.onNodeWithText("Alertmanager @alice → @bob", substring = true).assertExists()
    }

    @Test
    fun listsALinkByItsTitle() {
        server.send(
            alice,
            "bob",
            """{"summary": "CI failed",
                "link": [{"href": "https://ci.example.com/run/7", "title": "View run"}]}""",
        )
        opened("CI failed")
        compose.onNodeWithText("View run").assertExists()
    }

    @Test
    fun namesTheReplierByItsDisplayName() {
        server.portray(bob, "bob", "Bob Ross", png(8, 8))
        val id = server.send(alice, "bob", """{"summary": "Restart?", "choices": ["Yes"]}""")
        server.call("PUT", "/exchanges/$id/reply", bob, """{"choice": "Yes"}""")
        opened("Restart?")
        compose.awaitText("Bob Ross replied", substring = true)
    }
}
