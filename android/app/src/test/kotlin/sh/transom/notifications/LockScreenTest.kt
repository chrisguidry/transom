package sh.transom.notifications

import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.signin.app
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.send

/**
 * Which of a notification's actions need an unlocked phone: Open and Open link do by default, and
 * the setting for answering from the lock screen lifts that for them alone. Android requires it for
 * the reply box and its chips whatever the action says.
 */
@RunWith(RobolectricTestRunner::class)
class LockScreenTest {
    @get:Rule val server = TestServer()
    @get:Rule val connected = Connected()

    /** Whether each action of a notice's notification, by its title, needs an unlocked phone. */
    private fun unlocking(): Map<String, Boolean> {
        val alice = server.tokenFor("alice")
        server.accept("bob", server.tokenFor("bob"), alice)
        connected.signedIn(server, "bob")
        val notice = """{"summary": "Deploy?", "link": ["https://example.com"]}"""
        val id = server.send(alice, "bob", notice)
        return awaitPosted(id).actions.associate {
            it.title.toString() to it.isAuthenticationRequired
        }
    }

    @Test
    fun plainActionsNeedUnlockingByDefault() {
        assertEquals(mapOf("Reply" to false, "Open" to true, "Open link" to true), unlocking())
    }

    @Test
    fun theSettingLiftsItForPlainActionsOnly() {
        app.preferences.plainActionsWhileLocked = true
        assertEquals(mapOf("Reply" to false, "Open" to false, "Open link" to false), unlocking())
    }
}
