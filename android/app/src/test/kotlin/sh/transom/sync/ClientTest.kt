package sh.transom.sync

import java.io.IOException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
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
import sh.transom.testserver.send

@RunWith(RobolectricTestRunner::class)
class ClientTest {
    @get:Rule val server = TestServer()

    private lateinit var alice: String
    private lateinit var bob: String

    @Before
    fun people() {
        alice = server.tokenFor("alice")
        bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
    }

    private fun client(token: String) = Client(server.url, token)

    private fun asked(): String =
        server.send(
            alice,
            "bob",
            """{"summary": "Deploy?", "choices": [{"label": "Deploy", "id": "deploy"}, "Hold"]}""",
        )

    @Test
    fun aReplyIsAccepted() = runBlocking {
        val replied = client(bob).reply(asked(), choice = "deploy", text = "after 5pm")
        val exchange = (replied as Replied.Accepted).exchange
        assertEquals(
            listOf(State.REPLIED, "deploy", "after 5pm", "@bob"),
            listOf(
                exchange.state,
                exchange.reply?.choice,
                exchange.reply?.text,
                exchange.reply?.by,
            ),
        )
    }

    @Test
    fun theSameReplyAgainIsAccepted() = runBlocking {
        val id = asked()
        client(bob).reply(id, choice = "deploy")
        assertTrue(client(bob).reply(id, choice = "deploy") is Replied.Accepted)
    }

    @Test
    fun aDifferentReplyLosesToTheFirst() = runBlocking {
        val id = asked()
        client(bob).reply(id, choice = "deploy")
        val lost = client(bob).reply(id, text = "on second thought") as Replied.Lost
        assertEquals(listOf("deploy", "@bob"), listOf(lost.winner.choice, lost.winner.by))
    }

    @Test
    fun aReplyToAWithdrawnExchangeIsRefused() {
        val id = asked()
        server.call("DELETE", "/exchanges/$id/message", alice)
        val refused =
            assertThrows(Refused::class.java) { runBlocking { client(bob).reply(id, text = "ok") } }
        assertEquals(listOf(409, null), listOf(refused.status, refused.winner))
    }

    @Test
    fun theSenderWithdraws() = runBlocking {
        assertEquals(State.WITHDRAWN, client(alice).withdraw(asked()).state)
    }

    @Test
    fun marksReadAndUnread() = runBlocking {
        val id = asked()
        val marks = listOf(client(bob).mark(id, "bob", true), client(bob).mark(id, "bob", false))
        assertEquals(listOf(true, false), marks)
        assertEquals("""{"read":false}""", server.call("GET", "/exchanges/$id/@bob", bob))
    }

    @Test
    fun registersThisPhoneAsADeviceThatHoldsTheFeedOpen() = runBlocking {
        client(bob).register("bob", "phone-1", "Pixel 9")
        val devices = server.call("GET", "/@bob/devices", bob)
        assertTrue(
            devices,
            devices.contains("""{"id":"phone-1","name":"Pixel 9","wake":{"connection":true}}"""),
        )
    }

    @Test
    fun refusesAnotherPersonsList() {
        val refused =
            assertThrows(Refused::class.java) { runBlocking { client(bob).exchanges("alice") } }
        assertEquals(
            listOf(403, "an identity's exchanges are its own"),
            listOf(refused.status, refused.reason),
        )
    }

    @Test
    fun failsWhenTheServerCannotBeReached() {
        assertThrows(IOException::class.java) {
            runBlocking { Client(NOWHERE, bob).exchange("01JB") }
        }
    }
}
