package sh.transom.sync

import com.sun.net.httpserver.HttpServer
import java.io.IOException
import java.net.InetSocketAddress
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.produceIn
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.testserver.NOWHERE
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.group
import sh.transom.testserver.send

@RunWith(RobolectricTestRunner::class)
class FeedTest {
    @get:Rule val server = TestServer()

    private lateinit var alice: String
    private lateinit var bob: String

    @Before
    fun people() {
        alice = server.tokenFor("alice")
        bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
    }

    private suspend fun ReceiveChannel<FeedEvent>.next(): FeedEvent =
        withTimeout(10.seconds) { receive() }

    @Test
    fun opensAndThenSendsWhatChanges() = runBlocking {
        val events = Client(server.url, bob).feed("bob").produceIn(this)
        assertEquals(FeedEvent.Opened, events.next())
        val id = server.send(alice, "bob", """{"summary": "Deploy?", "prompt": ""}""")
        val changed = (events.next() as FeedEvent.Changed).exchange
        assertEquals(listOf(id, false), listOf(changed.id, changed.read))
        events.cancel()
    }

    @Test
    fun saysWhenAGroupsExchangeIsGone() = runBlocking {
        server.group("team", bob)
        val id = server.send(bob, "team", """{"summary": "Standup at 10"}""")
        val events = Client(server.url, bob).feed("bob").produceIn(this)
        assertEquals(FeedEvent.Opened, events.next())
        server.call("DELETE", "/@team/members/@bob", bob, expected = 204)
        assertEquals(FeedEvent.Gone(id), events.next())
        events.cancel()
    }

    @Test
    fun refusesAnotherPersonsFeed() {
        val refused =
            assertThrows(Refused::class.java) {
                runBlocking { Client(server.url, bob).feed("alice").first() }
            }
        assertEquals(403, refused.status)
    }

    @Test
    fun failsWhenTheServerCannotBeReached() {
        assertThrows(IOException::class.java) {
            runBlocking { Client(NOWHERE, bob).feed("bob").first() }
        }
    }

    /**
     * transom holds its feeds open for as long as it runs, but a proxy in front of it may end one,
     * as this server does after one event.
     */
    @Test
    fun endsWhenTheStreamEnds() = runBlocking {
        val proxy = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        proxy.createContext("/") { exchange ->
            exchange.responseHeaders.add("Content-Type", "text/event-stream")
            exchange.sendResponseHeaders(200, 0)
            exchange.responseBody.use {
                it.write("event: gone\ndata: {\"id\": \"01JB\"}\n\n".toByteArray())
            }
        }
        proxy.start()
        val events = Client("http://127.0.0.1:${proxy.address.port}", bob).feed("bob").toList()
        proxy.stop(0)
        assertEquals(listOf(FeedEvent.Opened, FeedEvent.Gone("01JB")), events)
    }
}
