package sh.transom.exchange

import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized
import sh.transom.sync.InboxEntry
import sh.transom.sync.Sending

/** Who an exchange is from and where it stands, in words, as @bob reads it and the web says it. */
@RunWith(Parameterized::class)
class MetaTest(private val entry: InboxEntry, private val line: String) {
    @Test fun saysWhoAndWhereItStands() = assertEquals(line, entry.meta("bob"))

    companion object {
        private val at = Instant.parse("2026-11-05T12:00:00Z")
        private val choices = listOf(Choice("Raise and restart", id = "raise"))

        private fun entry(
            state: State = State.OPEN,
            question: Boolean = false,
            from: String = "@alertmanager",
            to: String = "@bob",
            toName: String? = null,
            reply: Reply? = null,
            sending: Sending? = null,
        ) =
            InboxEntry(
                Exchange(
                    id = "x",
                    url = "/exchanges/x",
                    state = state,
                    question = question,
                    from = from,
                    fromName = "Alertmanager".takeIf { from == "@alertmanager" },
                    to = to,
                    toName = toName,
                    created = at,
                    message = Message(summary = "x", choices = choices),
                    reply = reply,
                ),
                false,
                sending,
            )

        @JvmStatic
        @Parameterized.Parameters(name = "{1}")
        fun cases() =
            listOf(
                arrayOf<Any>(entry(), "Alertmanager"),
                arrayOf<Any>(entry(to = "@ops", toName = "Ops"), "Alertmanager · to Ops"),
                arrayOf<Any>(
                    entry(
                        State.REPLIED,
                        true,
                        reply = Reply(choice = "raise", by = "@bob", at = at),
                    ),
                    "Alertmanager · You answered: Raise and restart",
                ),
                arrayOf<Any>(
                    entry(
                        State.REPLIED,
                        true,
                        reply = Reply(choice = "gone", by = "@anna", byName = "Anna", at = at),
                    ),
                    "Alertmanager · Anna answered: gone",
                ),
                arrayOf<Any>(
                    entry(
                        State.REPLIED,
                        true,
                        reply = Reply(text = "on it", by = "@carl", at = at),
                    ),
                    "Alertmanager · @carl answered: on it",
                ),
                arrayOf<Any>(entry(State.REPLIED, true), "Alertmanager · Answered"),
                arrayOf<Any>(entry(State.EXPIRED, true), "Alertmanager · Expired unanswered"),
                arrayOf<Any>(entry(State.EXPIRED), "Alertmanager · Expired"),
                arrayOf<Any>(entry(State.WITHDRAWN), "Alertmanager · Withdrawn"),
                arrayOf<Any>(entry(State.DECLINED), "Alertmanager · Declined"),
                arrayOf<Any>(entry(State.REPLACED), "Alertmanager · Replaced by a newer message"),
                arrayOf<Any>(
                    entry(question = true, sending = Sending("raise", null)),
                    "Alertmanager · Sending your answer…",
                ),
                arrayOf<Any>(
                    entry(question = true, from = "@bob", to = "@claude", toName = "Claude"),
                    "To Claude · Waiting for an answer",
                ),
                arrayOf<Any>(
                    entry(State.PENDING, from = "@bob", to = "@claude"),
                    "To @claude · Waiting to be accepted",
                ),
                arrayOf<Any>(entry(from = "@bob", to = "@claude"), "To @claude"),
            )
    }
}
