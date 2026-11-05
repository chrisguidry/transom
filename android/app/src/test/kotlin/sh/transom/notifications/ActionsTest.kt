package sh.transom.notifications

import android.app.Notification
import android.os.Looper
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import sh.transom.online
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.field
import sh.transom.testserver.send

/**
 * The buttons under @bob's notifications, three at most, by what each message asks of him, and
 * which of them need an unlocked phone.
 */
@RunWith(RobolectricTestRunner::class)
@Config(shadows = [ShadowFullScreen::class])
class ActionsTest {
    @get:Rule val server = TestServer()
    @get:Rule val connected = Connected()

    private lateinit var alice: String
    private lateinit var bob: String

    @Before
    fun people() {
        alice = server.tokenFor("alice")
        bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
        connected.signedIn(server, "bob")
    }

    private fun posted(json: String) = awaitPosted(server.send(alice, "bob", json))

    private val Notification.unlocking: Map<String, Boolean>
        get() = actions.associate { it.title.toString() to it.isAuthenticationRequired }

    @Test
    fun aMessageWithLinksOffersEachByItsTitle() =
        assertEquals(
            listOf("View run", "Logs"),
            posted(
                    """{"summary": "CI failed", "link": [{"href": "https://ci/7", "title": "View run"},
                        {"href": "https://ci/7/logs", "title": "Logs"}]}"""
                )
                .actionTitles,
        )

    @Test
    fun anAlertLeadsWithItsFirstLinkBeforeItsChoices() =
        assertEquals(
            listOf("Logs", "Silence 4h"),
            posted(
                    """{"summary": "Crash looping", "choices": ["Silence 4h"],
                        "link": [{"href": "https://grafana/explore", "title": "Logs"}]}"""
                )
                .actionTitles,
        )

    @Test
    fun anUrgentAlertLeadsWithGotItWhichNeedsNoUnlocking() =
        assertEquals(
            listOf("Got it" to false, "Silence 1h" to true, "Dashboard" to true),
            posted(
                    """{"summary": "UPS", "urgency": "urgent", "choices": ["Silence 1h"],
                        "link": [{"href": "https://grafana/d/ups", "title": "Dashboard"}]}"""
                )
                .unlocking
                .toList(),
        )

    @Test
    fun aQuestionOffersEachChoiceAsAButton() =
        assertEquals(
            listOf("Raise and restart", "Leave it"),
            posted("""{"summary": "Restart?", "choices": ["Raise and restart", "Leave it"]}""")
                .actionTitles,
        )

    @Test
    fun aQuestionWithAPromptTakesTextOnlyThen() {
        val posted = posted("""{"summary": "Why?", "prompt": "Say why"}""")
        assertEquals(
            listOf(listOf("Reply"), true, "Say why"),
            listOf(
                posted.actionTitles,
                posted.actions[0].remoteInputs[0].allowFreeFormInput,
                posted.actions[0].remoteInputs[0].label,
            ),
        )
    }

    @Test
    fun choicesThatDoNotFitBesideGotItAreAnsweredInTheApp() =
        assertEquals(
            listOf("Got it", "Answer…"),
            posted("""{"summary": "UPS", "urgency": "urgent", "choices": ["A", "B", "C"]}""")
                .actionTitles,
        )

    @Test
    fun aChoicesButtonAnswersWithIt() {
        val notification =
            posted("""{"summary": "Restart?", "choices": [{"label": "Yes", "id": "yes"}, "No"]}""")
        notification.actions[0].actionIntent.send()
        shadowOf(Looper.getMainLooper()).idle()
        await {
            online()
            server.call("GET", "/@bob/exchanges?state=replied", bob).contains("\"choice\":\"yes\"")
        }
    }

    @Test
    fun gotItReadsTheMessage() {
        val id = server.send(alice, "bob", """{"summary": "UPS", "urgency": "urgent"}""")
        awaitPosted(id).actions[0].actionIntent.send()
        shadowOf(Looper.getMainLooper()).idle()
        await {
            online()
            field(server.call("GET", "/exchanges/$id/@bob", bob), "read") == "true"
        }
    }
}
