package sh.transom.exchange

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.sync.Client
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.send

/** The types read what the real server writes. */
@RunWith(RobolectricTestRunner::class)
class ExchangeTest {
    @get:Rule val server = TestServer()

    private lateinit var alice: String
    private lateinit var bob: String

    @Before
    fun people() {
        alice = server.tokenFor("alice")
        bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
    }

    /** Alice asks Bob a question with every field a message has, and an image. */
    private fun asked(): String =
        server.send(
            alice,
            "bob",
            """
            {"summary": "Restart postgres?", "body": "The disk is full.\n\n![the graph](disk.png)",
             "choices": ["Leave it", {"label": "Restart", "id": "restart", "description": "Now."}],
             "prompt": "Why not?", "urgency": "high", "ttl": "1h",
             "link": [{"href": "https://grafana.example.com/d/disk", "rel": "related"}],
             "metadata": {"session": "abc"},
             "attachments": [{"name": "disk.png", "data": "$PNG"}]}
            """,
        )

    private fun asSeenBy(token: String, id: String): Exchange = runBlocking {
        Client(server.url, token).exchange(id)
    }

    @Test
    fun readsTheExchangeAroundTheMessage() {
        val id = asked()
        val exchange = asSeenBy(bob, id)
        assertEquals(
            listOf(id, "/exchanges/$id", State.OPEN, true, "@alice", "@bob", null),
            with(exchange) { listOf(this.id, url, state, question, from, to, reply) },
        )
        assertEquals(3600L, exchange.expires!!.epochSecond - exchange.created.epochSecond)
    }

    @Test
    fun readsEveryFieldOfTheMessage() {
        val message = asSeenBy(bob, asked()).message.copy(attachments = emptyList())
        assertEquals(
            Message(
                summary = "Restart postgres?",
                body = "The disk is full.\n\n![the graph](disk.png)",
                choices =
                    listOf(
                        Choice("Leave it"),
                        Choice("Restart", description = "Now.", id = "restart"),
                    ),
                prompt = "Why not?",
                ttl = 3600,
                links = listOf(Link("https://grafana.example.com/d/disk", "related")),
                urgency = Urgency.HIGH,
            ),
            message,
        )
    }

    @Test
    fun readsAnAttachmentWithItsUrl() {
        val id = asked()
        assertEquals(
            Attachment("disk.png", "image/png", 70, "/exchanges/$id/attachments/disk.png"),
            asSeenBy(bob, id).message.attachments.single(),
        )
    }

    @Test
    fun showsTheMetadataOnlyToTheSender() {
        val id = asked()
        assertEquals(
            listOf(mapOf("session" to "abc"), emptyMap()),
            listOf(asSeenBy(alice, id).message.metadata, asSeenBy(bob, id).message.metadata),
        )
    }

    @Test
    fun readsANotificationWithNothingButItsBody() {
        val exchange = asSeenBy(bob, server.send(alice, "bob", """{"body": "Backup finished"}"""))
        assertEquals(
            listOf(false, Message(body = "Backup finished"), null),
            listOf(exchange.question, exchange.message, exchange.expires),
        )
    }

    @Test
    fun skipsFieldsItDoesNotKnow() {
        val exchange =
            TransomJson.decodeFromString<Exchange>(
                """
                {"id": "1", "url": "/exchanges/1", "state": "open", "question": false,
                 "from": "@alice", "to": "@bob", "created": "2026-11-04T09:30:00.123Z",
                 "message": {"body": "Hi", "colour": "teal", "urgency": "urgent"},
                 "reply": null, "priority": 3}
                """
            )
        assertEquals(Message(body = "Hi"), exchange.message)
        assertNull(exchange.reply)
        assertNotNull(exchange.created)
    }
}

/** A one-pixel PNG of 70 bytes, in base64. */
private const val PNG =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
