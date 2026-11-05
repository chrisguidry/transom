package sh.transom.sync

import kotlin.time.Duration.Companion.milliseconds
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.testserver.TestServer
import sh.transom.testserver.send

/** The copy against a server that keeps an exchange for two seconds after it ends or arrives. */
@RunWith(RobolectricTestRunner::class)
class RetentionTest {
    @get:Rule val server = TestServer(mapOf("TRANSOM_RETENTION" to "2"))
    @get:Rule val memory = MemoryStore()

    private lateinit var bob: String
    private lateinit var sync: Sync

    @Before
    fun bob() {
        bob = server.tokenFor("bob")
        sync =
            Sync(
                Client(server.url, bob),
                memory.store,
                "bob",
                Backoff(first = 20.milliseconds, most = 200.milliseconds),
            )
    }

    private val inbox
        get() = memory.store.inbox("bob")

    private fun ids(entries: List<InboxEntry>) = entries.map { it.exchange.id }

    @Test
    fun aNotificationTheServerDeletesForItsAgeLeavesTheCopy() = runBlocking {
        val open = server.send(bob, "bob", """{"summary": "Deploy?", "choices": ["Deploy"]}""")
        val following = launch(Dispatchers.IO) { sync.follow() }
        inbox.awaiting { open in ids(it.entries) }
        val id = server.send(bob, "bob", """{"summary": "Backup finished"}""")
        inbox.awaiting { id in ids(it.entries) }
        inbox.awaiting { id !in ids(it.entries) && open in ids(it.entries) }
        following.cancel()
    }
}
