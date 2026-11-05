package sh.transom.sync

import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.testserver.NOWHERE
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.send

/** Mark all read goes through the outbox, so it reaches the server once the phone is online. */
@RunWith(RobolectricTestRunner::class)
class MarkAllReadTest {
    @get:Rule val server = TestServer()
    @get:Rule val memory = MemoryStore()

    private lateinit var bob: String
    private lateinit var told: List<String>
    private val outbox by lazy { Outbox(memory.store) {} }

    @Before
    fun told() = runBlocking {
        val alice = server.tokenFor("alice")
        bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
        told =
            listOf("Backup done", "Disk fine").map {
                server.send(alice, "bob", """{"summary": "$it"}""")
            }
        Sync(Client(server.url, bob), memory.store, "bob").refresh()
    }

    private suspend fun deliver(server: String = this.server.url) =
        outbox.deliver(Client(server, bob), "bob")

    private fun readOnServer() = told.map { server.call("GET", "/exchanges/$it/@bob", bob) }

    @Test
    fun marksEachReadOnTheServer() = runBlocking {
        outbox.markAll(told)
        assertEquals(true, deliver())
        assertEquals(listOf("""{"read":true}""", """{"read":true}"""), readOnServer())
    }

    @Test
    fun marksEachReadInTheCopyAtOnce() = runBlocking {
        outbox.markAll(told)
        val unread = memory.store.inbox("bob").first().unread.flatMap { it.ids }
        assertEquals(emptyList<String>(), told.filter { it in unread })
    }

    @Test
    fun staysQueuedWhileTheServerCannotBeReached() = runBlocking {
        outbox.markAll(told)
        assertEquals(false, deliver(server = NOWHERE))
        assertEquals(true, deliver())
        assertEquals(listOf("""{"read":true}""", """{"read":true}"""), readOnServer())
    }
}
