package sh.transom.sync

import androidx.test.core.app.ApplicationProvider
import java.time.Instant
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.exchange.Reply
import sh.transom.exchange.State

@RunWith(RobolectricTestRunner::class)
class StoreTest {
    @get:Rule val memory = MemoryStore()

    private val store
        get() = memory.store

    private val copies
        get() = store.database.copies()

    private suspend fun inbox() = store.inbox("bob").first()

    private fun ids(entries: List<InboxEntry>) = entries.map { it.exchange.id }

    // The exchanges here are from 1970, so whatever is not waiting or unread is older than a week.
    private fun rows(listings: List<Listing>) = listings.map { it.entry.exchange.id }

    @Test
    fun splitsTheInboxAsTheWebDoes() = runBlocking {
        copies.replace(
            listOf(
                exchange("asked", created = 5),
                exchange("told", question = false, created = 4),
                exchange("asking", from = "@bob", to = "@alice", read = null, created = 3),
                exchange("answered", state = State.REPLIED, read = true, created = 2),
                exchange("pending", state = State.PENDING, created = 1),
            )
        )
        val inbox = inbox()
        assertEquals(
            listOf(listOf("asked"), listOf("told", "pending"), listOf("asking", "answered")),
            listOf(ids(inbox.waiting), rows(inbox.unread), rows(inbox.older)),
        )
    }

    @Test
    fun ordersAsTheServerDoesNewestFirstThenById() = runBlocking {
        copies.replace(
            listOf(exchange("a", created = 1), exchange("c"), exchange("b", created = 1))
        )
        assertEquals(listOf("b", "a", "c"), ids(inbox().entries))
    }

    @Test
    fun listsWhatIsUnread() = runBlocking {
        copies.replace(
            listOf(
                exchange("new", question = false, read = false),
                exchange("seen", question = false, read = true),
                exchange("sent", from = "@bob", to = "@alice", read = null),
            )
        )
        assertEquals(listOf("new"), rows(inbox().unread))
    }

    @Test
    fun dropsWhatTheServerNoLongerLists() = runBlocking {
        copies.replace(listOf(exchange("kept"), exchange("left")))
        copies.replace(listOf(exchange("kept")))
        assertEquals(listOf("kept"), ids(inbox().entries))
    }

    @Test
    fun anExchangeThatEndedIsNotReopenedByALateDescription() = runBlocking {
        val replied =
            exchange("asked", state = State.REPLIED)
                .copy(reply = Reply(choice = "deploy", by = "@bob", at = Instant.ofEpochSecond(9)))
        copies.save(replied, "bob")
        copies.save(exchange("asked"), "bob")
        copies.replace(listOf(exchange("asked")))
        assertEquals(replied, inbox().entries.single().exchange)
    }

    @Test
    fun aLaterReplyReplacesOneStillQueued() = runBlocking {
        copies.replace(listOf(exchange("asked")))
        val outbox = Outbox(store) {}
        outbox.reply("asked", choice = "deploy")
        outbox.reply("asked", choice = "hold")
        assertEquals(Sending("hold", null), inbox().entries.single().sending)
    }

    @Test
    fun aLaterMarkReplacesOneStillQueued() = runBlocking {
        copies.replace(listOf(exchange("seen", read = false)))
        val outbox = Outbox(store) {}
        outbox.mark("seen", read = true)
        outbox.mark("seen", read = false)
        assertEquals(false, inbox().entries.single().read)
    }

    @Test
    fun showsAQueuedReplyAsSendingAndRead() = runBlocking {
        copies.replace(listOf(exchange("asked")))
        Outbox(store) {}.reply("asked", choice = "deploy")
        val inbox = inbox()
        assertEquals(
            listOf(
                emptyList(),
                listOf(InboxEntry(exchange("asked"), true, Sending("deploy", null))),
            ),
            listOf(inbox.waiting, inbox.older.map { it.entry }),
        )
    }

    @Test
    fun aQueuedMarkWinsOverTheCopy() = runBlocking {
        copies.replace(listOf(exchange("seen", read = true)))
        Outbox(store) {}.mark("seen", read = false)
        assertEquals(false, inbox().entries.single().read)
    }

    @Test
    fun wakesTheOutboxForEachThingQueued() = runBlocking {
        var wakes = 0
        val outbox = Outbox(store) { wakes++ }
        outbox.reply("asked", text = "yes")
        outbox.mark("asked", read = true)
        assertEquals(2, wakes)
    }

    @Test
    fun givesOneExchangeOrNothing() = runBlocking {
        copies.replace(listOf(exchange("asked")))
        assertEquals(exchange("asked"), store.exchange("asked").first()?.exchange)
        assertNull(store.exchange("missing").first())
    }

    @Test
    fun keepsOneDeviceId() = runBlocking {
        val device = store.database.device()
        assertEquals(device.id(), device.id())
    }

    @Test
    fun opensADatabaseOnThePhonesStorage() = runBlocking {
        val database = SyncDatabase.open(ApplicationProvider.getApplicationContext())
        database.copies().replace(listOf(exchange("asked")))
        assertEquals(listOf("asked"), ids(Store(database).inbox("bob").first().entries))
        database.close()
    }
}
