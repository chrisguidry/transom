package sh.transom.sync

import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.exchange.State
import sh.transom.testserver.NOWHERE
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.field
import sh.transom.testserver.group
import sh.transom.testserver.send

@RunWith(RobolectricTestRunner::class)
class OutboxTest {
    @get:Rule val server = TestServer()
    @get:Rule val memory = MemoryStore()

    private lateinit var alice: String
    private lateinit var bob: String
    private val outbox by lazy { Outbox(memory.store) {} }

    @Before
    fun people() {
        alice = server.tokenFor("alice")
        bob = server.tokenFor("bob")
    }

    /** Alice asks Bob, and Bob's copy has the question. */
    private suspend fun asked(): String {
        server.accept("bob", bob, alice)
        val id = server.send(alice, "bob", QUESTION)
        Sync(Client(server.url, bob), memory.store, "bob").refresh()
        return id
    }

    private suspend fun entry(id: String) = memory.store.exchange(id).first()!!

    private suspend fun deliver(token: String = bob, server: String = this.server.url) =
        outbox.deliver(Client(server, token), "bob")

    @Test
    fun aDeliveredReplyIsTheServers() = runBlocking {
        val id = asked()
        outbox.reply(id, choice = "deploy")
        assertEquals(true, deliver())
        val reply = server.call("GET", "/exchanges/$id/reply", alice)
        assertEquals(listOf("deploy", "@bob"), listOf(field(reply, "choice"), field(reply, "by")))
    }

    @Test
    fun aDeliveredReplySettlesTheCopy() = runBlocking {
        val id = asked()
        outbox.reply(id, choice = "deploy")
        deliver()
        val settled = entry(id)
        assertEquals(
            listOf(State.REPLIED, "deploy", null),
            listOf(settled.exchange.state, settled.exchange.reply?.choice, settled.sending),
        )
    }

    @Test
    fun theLoserGetsTheWinningReply() = runBlocking {
        server.group("team", alice, "bob" to bob)
        val id = server.send(alice, "team", QUESTION)
        Sync(Client(server.url, bob), memory.store, "bob").refresh()
        outbox.reply(id, choice = "hold")
        server.call("PUT", "/exchanges/$id/reply", alice, """{"choice": "deploy"}""")
        assertEquals(true, deliver())
        val settled = entry(id)
        assertEquals(
            listOf("@alice", "deploy", null),
            listOf(settled.exchange.reply?.by, settled.exchange.reply?.choice, settled.sending),
        )
    }

    @Test
    fun aReplyStaysQueuedWhileTheServerCannotBeReached() = runBlocking {
        val id = asked()
        outbox.reply(id, text = "on my way")
        assertEquals(false, deliver(server = NOWHERE))
        assertEquals(Sending(null, "on my way"), entry(id).sending)
    }

    @Test
    fun aReplyStaysQueuedUntilTheSessionIsSignedInAgain() = runBlocking {
        val id = asked()
        outbox.reply(id, text = "on my way")
        assertEquals(false, deliver(token = "transom_signed-out"))
        assertEquals(Sending(null, "on my way"), entry(id).sending)
    }

    @Test
    fun aReplyTheServerWillNeverTakeIsDropped() = runBlocking {
        val id = asked()
        outbox.reply(id, choice = "neither")
        assertEquals(true, deliver())
        val dropped = entry(id)
        assertEquals(listOf(State.OPEN, null), listOf(dropped.exchange.state, dropped.sending))
    }

    @Test
    fun aDeliveredMarkIsTheServers() = runBlocking {
        val id = asked()
        outbox.mark(id, read = true)
        assertEquals(true, deliver())
        assertEquals("""{"read":true}""", server.call("GET", "/exchanges/$id/@bob", bob))
        assertEquals(true, entry(id).read)
    }

    @Test
    fun aMarkStaysQueuedWhileTheServerCannotBeReached() = runBlocking {
        val id = asked()
        outbox.mark(id, read = true)
        assertEquals(false, deliver(server = NOWHERE))
        assertEquals(1, memory.store.database.queue().reads().size)
    }

    private companion object {
        const val QUESTION =
            """{"summary": "Deploy?", "choices": [{"label": "Deploy", "id": "deploy"}, {"label": "Hold", "id": "hold"}]}"""
    }
}
