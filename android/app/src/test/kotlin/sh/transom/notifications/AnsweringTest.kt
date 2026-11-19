package sh.transom.notifications

import android.app.Notification
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.online
import sh.transom.signin.app
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.field
import sh.transom.testserver.group
import sh.transom.testserver.send

/** @bob answers @alice's question from its notification, with a chip or with text. */
@RunWith(RobolectricTestRunner::class)
class AnsweringTest {
    @get:Rule val server = TestServer()
    @get:Rule val connected = Connected()

    private lateinit var alice: String
    private lateinit var bob: String

    @Before
    fun people() {
        alice = server.tokenFor("alice")
        bob = server.tokenFor("bob")
    }

    /** Alice asks Bob, with choices whose ids differ from their labels. */
    private fun asked(to: String = "bob"): String {
        if (to == "bob") server.accept("bob", bob, alice)
        connected.signedIn(server, "bob")
        return server.send(alice, to, DEPLOY)
    }

    /** The [part] of exchange [id]'s reply on the server, or `null` before there is one. */
    private fun replied(id: String, part: String): String? = runCatching {
        field(server.call("GET", "/exchanges/$id/reply", alice), part)
    }
        .getOrNull()

    @Test
    fun aChipRepliesWithItsChoicesKey() {
        val id = asked()
        awaitPosted(id).reply("Deploy now", chip = true)
        await {
            online()
            replied(id, "choice") == "deploy"
        }
    }

    @Test
    fun typedTextRepliesAsText() {
        val id = asked()
        awaitPosted(id).reply("after lunch")
        await {
            online()
            replied(id, "text") == "after lunch"
        }
    }

    @Test
    fun anEmptyReplySendsNothing() {
        val id = asked()
        awaitPosted(id).reply("  ")
        assertEquals(emptyList<Any>(), runBlocking { app.database.queue().replies() })
    }

    @Test
    fun theAnswerShowsFromMeWithNoActions() {
        val id = asked()
        awaitPosted(id).reply("Hold", chip = true)
        val sent = awaitPosted(id) { it.actions == null }
        assertEquals(listOf("@alice: Deploy?", "Me: Hold"), sent.messages)
        assertEquals(true, sent.silent)
    }

    @Test
    fun theNotificationClearsOnceTheServerTakesTheAnswer() {
        val id = asked()
        awaitPosted(id).reply("Hold", chip = true)
        awaitPosted(id) { it.actions == null }
        await {
            online()
            posted(id) == null
        }
    }

    /**
     * Bob answers Alice's question to the team, [id], from its notification after Alice has, and
     * the notification says what came of it.
     */
    private fun answeredAfterAlice(id: String): Notification {
        awaitPosted(id).reply("Deploy now", chip = true)
        server.call("PUT", "/exchanges/$id/reply", alice, """{"choice": "hold", "text": "wait"}""")
        return awaitPosted(id) {
            online()
            it.messages.size == 2 && it.messages.last().startsWith("@alice")
        }
    }

    @Test
    fun anAnswerThatWasNotUsedSaysSoAndWhoAnsweredFirst() {
        server.group("team", alice, "bob" to bob)
        val lost = answeredAfterAlice(asked(to = "team"))
        assertEquals(
            "@alice: Your answer, Deploy now, was not used: @alice answered first",
            lost.messages.last(),
        )
    }

    @Test
    fun swipingAwayAnAnswerThatWasNotUsedForgetsIt() {
        server.group("team", alice, "bob" to bob)
        val id = asked(to = "team")
        answeredAfterAlice(id).swipe()
        val store = checkNotNull(app.account()).store
        await {
            val entry = runBlocking { store.exchange(id).first() }
            entry?.read == true && entry.lost == null
        }
    }

    @Test
    fun swipingItAwayReadsIt() {
        val id = asked()
        awaitPosted(id).swipe()
        await {
            online()
            server.call("GET", "/exchanges/$id/@bob", bob).contains("true")
        }
    }

    private companion object {
        val DEPLOY =
            """{"summary": "Deploy?", "prompt": "", "choices": [
                {"label": "Deploy now", "id": "deploy"}, {"label": "Hold", "id": "hold"}]}"""
    }
}
