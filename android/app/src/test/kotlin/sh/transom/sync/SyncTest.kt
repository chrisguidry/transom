package sh.transom.sync

import kotlin.time.Duration.Companion.milliseconds
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.exchange.State
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.group
import sh.transom.testserver.send

@RunWith(RobolectricTestRunner::class)
class SyncTest {
    @get:Rule val server = TestServer()
    @get:Rule val memory = MemoryStore()

    private lateinit var alice: String
    private lateinit var bob: String
    private lateinit var sync: Sync

    @Before
    fun people() {
        alice = server.tokenFor("alice")
        bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
        sync = Sync(Client(server.url, bob), memory.store, "bob", quickly)
    }

    private val inbox
        get() = memory.store.inbox("bob")

    private fun asked(): String =
        server.send(alice, "bob", """{"summary": "Deploy?", "choices": ["Deploy", "Hold"]}""")

    private fun ids(entries: List<InboxEntry>) = entries.map { it.exchange.id }

    /**
     * Runs [test] while Bob's sync follows his feed, once its first refresh has copied exchange
     * [listed], so whatever reaches the copy after that came through the feed.
     */
    private fun whileFollowing(listed: String, test: suspend () -> Unit) = runBlocking {
        val following = launch(Dispatchers.IO) { sync.follow() }
        inbox.awaiting { listed in ids(it.entries) }
        test()
        following.cancel()
    }

    private suspend fun awaitState(id: String, state: State) = inbox.awaiting { inbox ->
        inbox.entries.any { it.exchange.id == id && it.exchange.state == state }
    }

    @Test
    fun aQuestionFromAliceWaitsOnBob() = runBlocking {
        val id = asked()
        sync.refresh()
        val inbox = inbox.first()
        assertEquals(listOf(listOf(id), true), listOf(ids(inbox.waiting), id in ids(inbox.unread)))
    }

    @Test
    fun aRefreshDropsWhatTheServerNoLongerLists() = runBlocking {
        server.group("team", bob)
        val id = server.send(bob, "team", """{"summary": "Standup at 10"}""")
        sync.refresh()
        server.call("DELETE", "/@team/members/@bob", bob, expected = 204)
        sync.refresh()
        assertEquals(false, id in ids(inbox.first().entries))
    }

    @Test
    fun aRefreshCopiesEveryExchangeTheServerKeeps() = runBlocking {
        val sent =
            (1..201).map { server.send(alice, "bob", """{"summary": "Backup $it finished"}""") }
        sync.refresh()
        val copied = ids(inbox.first().entries)
        assertEquals(sent, sent.filter { it in copied })
    }

    @Test
    fun aQuestionAskedWhileFollowingReachesTheCopy() =
        whileFollowing(asked()) {
            val id = asked()
            inbox.awaiting { id in ids(it.waiting) }
        }

    @Test
    fun aWithdrawalReachesTheCopy() {
        val id = asked()
        whileFollowing(id) {
            server.call("DELETE", "/exchanges/$id/message", alice)
            awaitState(id, State.WITHDRAWN)
        }
    }

    @Test
    fun anExpiryReachesTheCopy() {
        val id = server.send(alice, "bob", """{"summary": "Deploy?", "prompt": "", "ttl": 2}""")
        whileFollowing(id) { awaitState(id, State.EXPIRED) }
    }

    @Test
    fun aDeletedGroupsExchangesAreDropped() {
        server.group("team", bob)
        val id = server.send(bob, "team", """{"summary": "Standup at 10"}""")
        whileFollowing(id) {
            server.call("DELETE", "/@team/members/@bob", bob, expected = 204)
            inbox.awaiting { id !in ids(it.entries) }
        }
    }

    @Test
    fun readingOnAnotherDeviceReachesTheCopy() {
        val id = asked()
        whileFollowing(id) {
            server.call("PUT", "/exchanges/$id/@bob", bob)
            inbox.awaiting { inbox ->
                inbox.entries.any { it.exchange.id == id && it.read == true }
            }
        }
    }

    @Test
    fun theCopyCatchesUpAfterTheServerComesBack() {
        val id = asked()
        whileFollowing(id) {
            server.restart()
            server.call("DELETE", "/exchanges/$id/message", alice)
            awaitState(id, State.WITHDRAWN)
        }
    }

    @Test
    fun theSenderWithdraws() = runBlocking {
        val id = asked()
        val asAlice = Sync(Client(server.url, alice), memory.store, "alice")
        asAlice.refresh()
        asAlice.withdraw(id)
        assertEquals(State.WITHDRAWN, memory.store.exchange(id).first()?.exchange?.state)
    }

    @Test
    fun registersThisPhoneOnceUnderOneId() = runBlocking {
        sync.register("Pixel 9")
        sync.register("Pixel 9")
        val devices = server.call("GET", "/@bob/devices", bob)
        assertEquals(1, Regex(""""name":"Pixel 9"""").findAll(devices).count())
    }

    private companion object {
        val quickly = Backoff(first = 20.milliseconds, most = 200.milliseconds)
    }
}
