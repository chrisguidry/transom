package sh.transom.exchange

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isHeading
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
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
import sh.transom.testserver.send

/**
 * A message's own images, fetched with the session's token from a real server. Robolectric's native
 * graphics decode them as a phone does.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class AttachmentsTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()

    /** @alice sends @bob [body] with [attachments], and Bob opens it. */
    private fun opened(body: String, vararg attachments: Pair<String, ByteArray>) {
        val alice = server.tokenFor("alice")
        val bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
        server.send(alice, "bob", message("Look", body = body, attachments = attachments.toList()))
        launched.signedIn(server, "bob")
        compose.awaitText("Look")
        compose.onNodeWithText("Look").performClick()
        compose.awaitMain {
            compose.onAllNodes(hasText("Look") and isHeading()).fetchSemanticsNodes().isNotEmpty()
        }
    }

    private fun awaitImage(description: String) = compose.awaitMain {
        compose.onAllNodes(hasContentDescription(description)).fetchSemanticsNodes().isNotEmpty()
    }

    private fun top(text: String) = compose.onNodeWithText(text).getBoundsInRoot().top

    private fun imageTop(description: String) =
        compose.onNodeWithContentDescription(description).getBoundsInRoot().top

    @Test
    fun showsAnImageWhereTheBodyPlacesIt() {
        opened("Before\n\n![the graph](graph.png)\n\nAfter", "graph.png" to png(40, 20))
        awaitImage("the graph")
        val order = listOf(top("Before"), imageTop("the graph"), top("After"))
        assertEquals(order.sorted(), order)
    }

    @Test
    fun showsAnImageTheBodyDoesNotPlaceAfterIt() {
        opened("The body", "graph.png" to png(40, 20))
        awaitImage("graph.png")
        assertEquals(true, imageTop("graph.png") > top("The body"))
    }

    @Test
    fun showsAnImageAtItsOwnWidthInAWideColumn() {
        opened("", "graph.png" to png(40, 20))
        awaitImage("graph.png")
        val bounds = compose.onNodeWithContentDescription("graph.png").getBoundsInRoot()
        val size = listOf(bounds.right - bounds.left, bounds.bottom - bounds.top).map { it.value }
        assertEquals(listOf(40f, 20f), size)
    }

    @Test
    fun namesAnImageThatCannotBeShown() {
        opened(
            "",
            "broken.png" to "\u0089PNG\r\n\u001a\n but no more".toByteArray(Charsets.ISO_8859_1),
        )
        compose.awaitText("broken.png")
        compose.onNodeWithText("broken.png").assertIsDisplayed()
    }
}
