package sh.transom.notifications

import android.app.KeyguardManager
import android.content.Intent
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ActivityController
import sh.transom.MainActivity
import sh.transom.awaitOnline
import sh.transom.signin.app
import sh.transom.signin.awaitMain
import sh.transom.signin.awaitText
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.field
import sh.transom.testserver.send

/** An urgent question full screen, over the lock screen, as @bob's phone shows it. */
@RunWith(RobolectricTestRunner::class)
class AlertActivityTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val connected = Connected()

    private lateinit var alice: String
    private lateinit var bob: String
    private lateinit var id: String

    @Before
    fun paged() {
        alice = server.tokenFor("alice")
        bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
        connected.signedIn(server, "bob")
        id =
            server.send(
                alice,
                "bob",
                """{"summary": "UPS on battery", "body": "The rack is on battery.",
                    "urgency": "urgent", "choices": [{"label": "Shut down", "id": "down"}],
                    "link": [{"href": "https://grafana.example.com/d/ups", "title": "Dashboard"}]}""",
            )
        awaitPosted(id)
    }

    private fun page(exchange: String? = id): ActivityController<AlertActivity> {
        val intent = Intent(app, AlertActivity::class.java)
        exchange?.let { intent.putExtra(AlertActivity.EXCHANGE, it) }
        return Robolectric.buildActivity(AlertActivity::class.java, intent).setup()
    }

    private val keyguard
        get() = shadowOf(app.getSystemService(KeyguardManager::class.java))

    private fun reply() = server.call("GET", "/exchanges/$id", bob).let { field(it, "state") }

    @Test
    fun showsTheMessageWithAcknowledgeItsChoicesAndItsLink() {
        page()
        compose.awaitText("UPS on battery")
        listOf("The rack is on battery.", "Acknowledge", "Shut down", "Dashboard").forEach {
            compose.onNodeWithText(it).assertExists()
        }
    }

    @Test
    fun acknowledgeReadsItAndCloses() {
        val shown = page()
        compose.awaitText("Acknowledge")
        compose.onNodeWithText("Acknowledge").performClick()
        compose.awaitOnline {
            server.call("GET", "/exchanges/$id/@bob", bob) == """{"read":true}"""
        }
        compose.awaitMain { shown.get().isFinishing }
    }

    @Test
    fun aChoiceAnswersOnceThePhoneIsUnlocked() {
        keyguard.setKeyguardLocked(true)
        page()
        compose.awaitText("Shut down")
        compose.onNodeWithText("Shut down").performClick()
        assertEquals("open", reply())
        keyguard.setKeyguardLocked(false)
        compose.awaitOnline { reply() == "replied" }
    }

    @Test
    fun theLinkOpensThroughTransomsOwnActivity() {
        val shown = page()
        compose.awaitText("Dashboard")
        compose.onNodeWithText("Dashboard").performClick()
        val opening = shadowOf(shown.get()).nextStartedActivity
        assertEquals(
            listOf(LinkActivity::class.java.name, "https://grafana.example.com/d/ups"),
            listOf(opening.component?.className, opening.dataString),
        )
    }

    @Test
    fun openInTransomOpensTheExchangeInTheApp() {
        val shown = page()
        compose.awaitText("Open in transom")
        compose.onNodeWithText("Open in transom").performClick()
        val opening = shadowOf(shown.get()).nextStartedActivity
        assertEquals(
            listOf(MainActivity::class.java.name, id),
            listOf(opening.component?.className, opening.getStringExtra(OPEN_EXCHANGE)),
        )
    }

    @Test
    fun closesOnceReadElsewhere() {
        val shown = page()
        compose.awaitText("UPS on battery")
        server.call("PUT", "/exchanges/$id/@bob", bob)
        compose.awaitMain { shown.get().isFinishing }
    }

    @Test
    fun aMessageWithNothingToAnswerOffersAcknowledgeAlone() {
        val door =
            server.send(alice, "bob", """{"summary": "The door is open", "urgency": "urgent"}""")
        awaitPosted(door)
        page(door)
        compose.awaitText("The door is open")
        // Acknowledge, and Open in transom, and no choice or link.
        assertEquals(2, compose.onAllNodes(hasClickAction()).fetchSemanticsNodes().size)
    }

    @Test
    fun closesWithoutAnExchange() {
        assertEquals(true, page(exchange = null).get().isFinishing)
    }
}
