package sh.transom.inbox

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasAnySibling
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performCustomAccessibilityActionWithLabel
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.GraphicsMode
import sh.transom.LaunchedApp
import sh.transom.awaitOnline
import sh.transom.exchange.png
import sh.transom.signin.awaitMain
import sh.transom.signin.awaitText
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.portray
import sh.transom.testserver.send

/** @bob reads what is in his inbox without opening it, and sees who sent each. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class InboxReadingTest {
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

    private fun told(summary: String) = server.send(alice, "bob", """{"summary": "$summary"}""")

    private fun read(id: String) = server.call("GET", "/exchanges/$id/@bob", bob)

    private fun opened(summary: String) {
        launched.signedIn(server, "bob")
        compose.awaitText(summary)
    }

    @Test
    fun markAllReadReadsEveryUnreadExchange() {
        val ids = listOf(told("Backup done"), told("Disk fine"))
        opened("Disk fine")
        compose.onNodeWithText("Mark all read").performClick()
        compose.awaitOnline { ids.all { read(it) == """{"read":true}""" } }
    }

    @Test
    fun swipingARowReadsIt() {
        val id = told("Backup done")
        opened("Backup done")
        compose.onNode(hasText("Backup done")).performTouchInput { swipeLeft() }
        compose.awaitOnline { read(id) == """{"read":true}""" }
    }

    @Test
    fun aRowAnswersAShortQuestionInATap() {
        val id =
            server.send(
                alice,
                "bob",
                """{"summary": "Restart?", "choices": [{"label": "Raise and restart", "id": "raise"}]}""",
            )
        opened("Restart?")
        compose.onNodeWithText("Raise and restart").performClick()
        compose.awaitOnline {
            server.call("GET", "/exchanges/$id", bob).contains(""""choice":"raise"""")
        }
    }

    @Test
    fun undoPutsARowBackUnread() {
        val id = told("Backup done")
        opened("Backup done")
        compose.onNode(hasText("Backup done")).performTouchInput { swipeLeft() }
        compose.awaitOnline { read(id) == """{"read":true}""" }
        compose.onNodeWithText("Undo").performClick()
        compose.awaitOnline { read(id) == """{"read":false}""" }
    }

    @Test
    @OptIn(ExperimentalTestApi::class)
    fun aScreenReaderMarksARowReadWithoutASwipe() {
        val id = told("Backup done")
        opened("Backup done")
        compose
            .onNode(hasText("Backup done"))
            .performCustomAccessibilityActionWithLabel("Mark read")
        compose.awaitOnline { read(id) == """{"read":true}""" }
    }

    @Test
    fun aReadRowIsTwoLinesWithItsMetaUnderTheTitle() {
        val id = told("Backup done")
        server.call("PUT", "/exchanges/$id/@bob", bob)
        opened("Backup done")
        val bounds = { node: SemanticsMatcher ->
            compose.onNode(node, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot
        }
        val title = bounds(hasText("Backup done"))
        val meta = bounds(hasText("@alice") and hasAnySibling(hasText("Backup done")))
        assertEquals(true, meta.top >= title.bottom)
    }

    @Test
    fun aMessageAndItsReplacementAreOneRow() {
        server.send(alice, "bob", """{"summary": "Disk full", "key": "disk"}""")
        server.send(alice, "bob", """{"summary": "Disk fine", "key": "disk"}""")
        opened("Disk fine")
        assertEquals(0, compose.onAllNodes(hasText("Disk full")).fetchSemanticsNodes().size)
    }

    @Test
    fun namesTheSenderByItsDisplayName() {
        server.portray(alice, "alice", "Alertmanager", png(8, 8))
        told("Postgres is down")
        opened("Postgres is down")
        compose.awaitMain {
            compose.onAllNodes(hasText("Alertmanager")).fetchSemanticsNodes().isNotEmpty()
        }
    }

    @Test
    fun showsTheFirstLineOfTheBodyUnderTheSummary() {
        server.send(
            alice,
            "bob",
            """{"summary": "Postgres is down", "body": "\npaperless-db-1 has not answered."}""",
        )
        opened("Postgres is down")
        compose.onNodeWithText("paperless-db-1 has not answered.").assertExists()
    }
}
