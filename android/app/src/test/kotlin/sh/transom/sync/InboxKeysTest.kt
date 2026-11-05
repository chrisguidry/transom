package sh.transom.sync

import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.exchange.State

/** The inbox lists only the latest message of each key, and Mark all read marks what it hides. */
@RunWith(RobolectricTestRunner::class)
class InboxKeysTest {
    @get:Rule val memory = MemoryStore()

    private val copies
        get() = memory.store.database.copies()

    private suspend fun inbox() = memory.store.inbox("bob").first()

    private fun ids(entries: List<InboxEntry>) = entries.map { it.exchange.id }

    private fun told(id: String, state: State = State.OPEN, read: Boolean = false, created: Long) =
        exchange(id, question = false, state = state, read = read, created = created)

    @Test
    fun aReplacedExchangeIsNotListed() = runBlocking {
        copies.replace(
            listOf(
                told("resolved", created = 2).copy(replaces = "firing"),
                told("firing", state = State.REPLACED, created = 1),
            )
        )
        assertEquals(listOf("resolved"), ids(inbox().unread.map { it.entry }))
    }

    @Test
    fun anExchangeALaterOneReplacesIsNotListedBeforeItsStateArrives() = runBlocking {
        copies.replace(
            listOf(
                told("resolved", created = 2).copy(replaces = "firing"),
                told("firing", created = 1),
            )
        )
        assertEquals(listOf("resolved"), ids(inbox().unread.map { it.entry }))
    }

    @Test
    fun markAllReadMarksTheUnreadAndTheReplacedNeverRead() = runBlocking {
        copies.replace(
            listOf(
                told("resolved", created = 3).copy(replaces = "firing"),
                told("firing", state = State.REPLACED, created = 2),
                told("seen", read = true, created = 1),
                exchange("asked", created = 0),
            )
        )
        assertEquals(listOf("resolved", "firing"), inbox().markable)
    }
}
