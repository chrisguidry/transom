package sh.transom.sync

import java.time.Instant
import sh.transom.exchange.Exchange
import sh.transom.exchange.Message
import sh.transom.exchange.State

/**
 * An exchange as the server would describe it, for the tests of the copy that need no server.
 * [created] counts seconds, so a larger one is newer.
 */
fun exchange(
    id: String,
    from: String = "@alice",
    to: String = "@bob",
    state: State = State.OPEN,
    question: Boolean = true,
    read: Boolean? = false,
    created: Long = 0,
): Exchange =
    Exchange(
        id = id,
        url = "/exchanges/$id",
        state = state,
        question = question,
        from = from,
        to = to,
        created = Instant.ofEpochSecond(created),
        message = Message(summary = id),
        read = read,
    )
