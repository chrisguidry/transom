package sh.transom.exchange

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.sync.Client
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.portray
import sh.transom.testserver.send

/** The fields an alert uses read as the real server writes them. */
@RunWith(RobolectricTestRunner::class)
class AlertFieldsTest {
    @get:Rule val server = TestServer()

    private lateinit var alice: String
    private lateinit var bob: String

    @Before
    fun people() {
        alice = server.tokenFor("alice")
        bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
    }

    private fun seen(id: String): Exchange = runBlocking { Client(server.url, bob).exchange(id) }

    @Test
    fun readsAnUrgentMessageWithItsKeyAndLinkTitles() {
        val id =
            server.send(
                alice,
                "bob",
                """{"summary": "UPS on battery", "urgency": "urgent", "key": "ups",
                    "link": [{"href": "https://grafana.example.com/d/ups", "title": "Dashboard"}]}""",
            )
        val message = seen(id).message
        assertEquals(
            listOf(
                Urgency.URGENT,
                "ups",
                Link("https://grafana.example.com/d/ups", title = "Dashboard"),
            ),
            listOf(message.urgency, message.key, message.links.single()),
        )
    }

    @Test
    fun aMessageWithTheSameKeyReplacesTheOpenOne() {
        val firing = server.send(alice, "bob", """{"summary": "Disk full", "key": "disk"}""")
        val resolved = server.send(alice, "bob", """{"summary": "Disk fine", "key": "disk"}""")
        assertEquals(
            listOf(State.REPLACED, firing),
            listOf(seen(firing).state, seen(resolved).replaces),
        )
    }

    @Test
    fun anExchangeThatNeverRepeatedHasNoRepeats() =
        assertEquals(0, seen(server.send(alice, "bob", """{"summary": "Hi"}""")).repeats)

    @Test
    fun namesEachPartyByItsDisplayNameAndPicture() {
        server.portray(alice, "alice", "Alice Liddell", png(8, 8))
        val id = server.send(alice, "bob", """{"summary": "Deploy?", "choices": ["Yes"]}""")
        server.call("PUT", "/exchanges/$id/reply", bob, """{"choice": "Yes"}""")
        val exchange = seen(id)
        assertEquals(
            listOf("Alice Liddell", null, null),
            listOf(exchange.fromName, exchange.toName, exchange.toPicture),
        )
        assertTrue(exchange.fromPicture!!.contains("/@alice/picture?v="))
        assertEquals(listOf(null, null), listOf(exchange.reply!!.byName, exchange.reply.byPicture))
    }
}
