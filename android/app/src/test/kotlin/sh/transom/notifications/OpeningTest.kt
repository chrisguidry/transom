package sh.transom.notifications

import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isHeading
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import sh.transom.LaunchedApp
import sh.transom.MainActivity
import sh.transom.signin.awaitMain
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.send

/** Tapping a notification, or its Open, opens its exchange in the app. */
@RunWith(RobolectricTestRunner::class)
class OpeningTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()
    @get:Rule val connected = Connected()

    @Test
    fun tappingANotificationOpensItsExchange() {
        val alice = server.tokenFor("alice")
        val bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
        val activity = launched.signedIn(server, "bob")
        connected.start()
        val id = server.send(alice, "bob", """{"summary": "Deploy?", "choices": ["Deploy"]}""")
        val tap = shadowOf(awaitPosted(id).contentIntent).savedIntent
        assertEquals(MainActivity::class.java.name, tap.component?.className)
        activity.newIntent(tap)
        compose.awaitMain {
            compose
                .onAllNodes(hasText("Deploy?") and isHeading())
                .fetchSemanticsNodes()
                .isNotEmpty()
        }
        // Opening it reads it, which clears its notification.
        awaitCleared(id)
    }
}
