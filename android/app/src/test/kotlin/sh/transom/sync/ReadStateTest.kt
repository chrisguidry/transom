package sh.transom.sync

import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import sh.transom.exchange.Exchange

/**
 * Whether Bob has a read state on an exchange the server described outside the list: the feed gives
 * `read` to everyone, and `GET /exchanges/{id}` to no one.
 */
@RunWith(ParameterizedRobolectricTestRunner::class)
class ReadStateTest(
    private val case: String,
    private val listed: Exchange?,
    private val described: Exchange,
    private val read: Boolean?,
) {
    @get:Rule val memory = MemoryStore()

    @Test
    fun keepsAReadStateOnlyForARecipient() = runBlocking {
        val copies = memory.store.database.copies()
        listed?.let { copies.replace(listOf(it)) }
        copies.save(described, "bob")
        assertEquals(read, memory.store.inbox("bob").first().entries.single().read)
    }

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun cases() =
            listOf(
                arrayOf<Any?>("sent to him", null, exchange("x", read = false), false),
                arrayOf<Any?>(
                    "sent by him",
                    null,
                    exchange("x", from = "@bob", to = "@alice"),
                    null,
                ),
                arrayOf<Any?>(
                    "sent to himself",
                    null,
                    exchange("x", from = "@bob", to = "@bob"),
                    false,
                ),
                arrayOf<Any?>("listed as sent", exchange("x", read = null), exchange("x"), null),
                arrayOf<Any?>(
                    "read, then got",
                    exchange("x", read = true),
                    exchange("x", read = null),
                    true,
                ),
                arrayOf<Any?>(
                    "then read",
                    exchange("x", read = false),
                    exchange("x", read = true),
                    true,
                ),
            )
    }
}
