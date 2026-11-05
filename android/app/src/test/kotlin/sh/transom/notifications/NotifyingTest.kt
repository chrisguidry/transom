package sh.transom.notifications

import androidx.core.app.NotificationManagerCompat
import androidx.core.content.pm.ShortcutManagerCompat
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import sh.transom.exchange.message
import sh.transom.signin.app
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.field
import sh.transom.testserver.send

/** What @alice sends reaches @bob's notifications, and each clears when it should. */
@RunWith(RobolectricTestRunner::class)
class NotifyingTest {
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

    private fun ask(
        json: String = message("Deploy?", choices = listOf("Deploy" to null, "Hold" to null))
    ) = server.send(alice, "bob", json)

    @Test
    fun aQuestionIsAConversationWithItsSender() {
        val id = ask(message("Deploy?", body = "The build is **green**."))
        val posted = awaitPosted(id)
        assertEquals(listOf("@alice: Deploy?\nThe build is green."), posted.messages)
        assertEquals("@alice", posted.shortcutId)
        val shortcut = ShortcutManagerCompat.getDynamicShortcuts(app).single()
        assertEquals("@alice", shortcut.id)
    }

    @Test
    fun aQuestionAlertsOnItsUrgencysChannel() {
        val id = ask("""{"summary": "Disk?", "prompt": "", "urgency": "high"}""")
        val posted = awaitPosted(id)
        assertEquals("high", posted.channelId)
        assertEquals(false, posted.silent)
    }

    @Test
    fun aNotificationPostedBeforeARestartIsPostedAgainWithoutASound() {
        val id = ask()
        awaitPosted(id)
        // A restarted phone shows none of the notifications it had, and a new process remembers
        // nothing it posted but what is in the database.
        NotificationManagerCompat.from(app).cancelAll()
        val account = app.account()!!
        val restarted = Notifier(app, account.store, account.outbox, "bob", account.client)
        account.background { restarted.follow() }
        assertTrue(awaitPosted(id).silent)
    }

    @Test
    fun replyComesFirstWithThePromptAndTheChoicesAsChips() {
        val id =
            ask(
                """{"summary": "Deploy?", "prompt": "Why?", "choices": ["Deploy", "Hold"],
                    "link": [{"href": "https://ci.example.com/run/7"}]}"""
            )
        val posted = awaitPosted(id)
        assertEquals(listOf("Reply", "Open link"), posted.actionTitles)
        assertEquals("Why?", posted.actions[0].remoteInputs[0].label)
        assertEquals(listOf("Deploy", "Hold"), posted.chips)
        assertTrue(posted.actions[0].remoteInputs[0].allowFreeFormInput)
    }

    @Test
    fun openLinkNamesTransomsOwnActivity() {
        val id =
            ask("""{"summary": "Deploy?", "link": [{"href": "https://ci.example.com/run/7"}]}""")
        val opening = shadowOf(awaitPosted(id).actions.last().actionIntent).savedIntent
        assertEquals(LinkActivity::class.java.name, opening.component?.className)
        assertEquals("https://ci.example.com/run/7", opening.dataString)
    }

    @Test
    fun choicesThatMayNotFitAreLeftToTheApp() {
        val id = ask(message("Pick", choices = (1..5).map { "Choice $it" to null }))
        assertEquals(listOf("Answer…"), awaitPosted(id).actionTitles)
    }

    @Test
    fun theNotificationClearsWhenAliceWithdraws() {
        val id = ask()
        awaitPosted(id)
        server.call("DELETE", "/exchanges/$id/message", alice)
        awaitCleared(id)
    }

    @Test
    fun theNotificationClearsWhenBobReadsItElsewhere() {
        val id = ask()
        awaitPosted(id)
        server.call("PUT", "/exchanges/$id/@bob", bob)
        awaitCleared(id)
    }

    @Test
    fun theNotificationClearsWhenBobAnswersElsewhere() {
        val id = ask()
        awaitPosted(id)
        server.call("PUT", "/exchanges/$id/reply", bob, """{"choice": "Hold"}""")
        awaitCleared(id)
    }

    @Test
    fun aDeadlineTimesTheNotificationOutAndClearsIt() {
        val id = ask("""{"summary": "Now?", "choices": ["Yes"], "ttl": 2}""")
        val posted = awaitPosted(id)
        assertTrue("${posted.timeoutAfter}", posted.timeoutAfter in 1..2_000)
        awaitCleared(id)
    }

    @Test
    fun aOneWayNotificationThatArrivesIsPosted() {
        val id = ask("""{"summary": "Backup done"}""")
        assertEquals(listOf("@alice: Backup done"), awaitPosted(id).messages)
    }

    @Test
    fun aOneWayNotificationFromBeforeIsNot() {
        // Alice's first message to Bob, held until he accepted her, arrived before notifying began.
        val listed = server.call("GET", "/@bob/exchanges?question=false", bob)
        val hello =
            field(
                Json.parseToJsonElement(listed)
                    .jsonObject["exchanges"]!!
                    .jsonArray
                    .single()
                    .toString(),
                "id",
            )
        awaitPosted(ask("""{"summary": "Later"}"""))
        assertNull(posted(hello))
    }
}
