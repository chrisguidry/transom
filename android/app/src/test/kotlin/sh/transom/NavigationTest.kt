package sh.transom

import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.hasScrollToNodeAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isHeading
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.android.controller.ActivityController
import sh.transom.signin.awaitMain
import sh.transom.signin.awaitText
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.send

/** From @bob's inbox to an exchange and back, by the system's back gesture or the wordmark. */
@RunWith(RobolectricTestRunner::class)
class NavigationTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()

    private lateinit var activity: ActivityController<MainActivity>
    private lateinit var alice: String

    @Before
    fun openAnExchange() {
        alice = server.tokenFor("alice")
        val bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
        server.send(alice, "bob", """{"summary": "Deploy?", "choices": [{"label": "Deploy"}]}""")
        activity = launched.signedIn(server, "bob")
        compose.awaitText("Deploy?")
        compose.onNodeWithText("Deploy?").performClick()
        awaitHeading("Deploy?")
    }

    private fun awaitHeading(text: String) = compose.awaitMain {
        compose.onAllNodes(hasText(text) and isHeading()).fetchSemanticsNodes().isNotEmpty()
    }

    @Test
    fun backReturnsToTheInbox() {
        activity.get().onBackPressedDispatcher.onBackPressed()
        awaitHeading("@bob")
    }

    @Test
    fun theInboxKeepsItsPlaceWhileAnExchangeIsOpen() {
        activity.get().onBackPressedDispatcher.onBackPressed()
        awaitHeading("@bob")
        for (number in 1..15) server.send(alice, "bob", """{"summary": "Notice $number"}""")
        compose.awaitText("Notice 15")
        compose.onNode(hasScrollToNodeAction()).performScrollToNode(hasText("Notice 1"))
        val before = compose.onNodeWithText("Notice 1").getBoundsInRoot().top
        compose.onNodeWithText("Notice 1").performClick()
        awaitHeading("Notice 1")
        activity.get().onBackPressedDispatcher.onBackPressed()
        compose.awaitText("Notice 1")
        assertEquals(before, compose.onNodeWithText("Notice 1").getBoundsInRoot().top)
    }

    @Test
    fun theWordmarkReturnsToTheInbox() {
        compose.onNodeWithText("transom").performClick()
        awaitHeading("@bob")
    }
}
