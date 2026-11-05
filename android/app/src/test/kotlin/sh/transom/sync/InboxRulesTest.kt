package sh.transom.sync

import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Test
import sh.transom.exchange.Choice
import sh.transom.exchange.Exchange
import sh.transom.exchange.Link
import sh.transom.exchange.Message
import sh.transom.exchange.State

/** Which exchanges @bob's inbox lists, and under which section, by the rules the web's follows. */
class InboxRulesTest {
    private val now = Instant.parse("2026-11-05T12:00:00Z")

    /** An exchange to @bob, unread, that arrived [minutes] before [now], with [message]. */
    private fun entry(
        id: String,
        minutes: Long = 1,
        message: Message = Message(summary = id),
        question: Boolean = false,
        from: String = "@alertmanager",
        state: State = State.OPEN,
        read: Boolean? = false,
        sending: Sending? = null,
    ) =
        InboxEntry(
            Exchange(
                id = id,
                url = "/exchanges/$id",
                state = state,
                question = question,
                from = from,
                to = "@bob",
                created = now.minusSeconds(minutes * 60),
                message = message,
            ),
            read,
            sending,
        )

    private fun inbox(vararg entries: InboxEntry) = Inbox("bob", entries.toList(), now)

    private fun rows(listings: List<Listing>) = listings.map { it.entry.exchange.id }

    private val choices = listOf(Choice("Restart"))
    private val link = listOf(Link("https://grafana/d"))

    @Test
    fun aQuestionWaitsOnlyWhenItAsksForAnAnswer() {
        val inbox =
            inbox(
                entry("only-choices", message = Message(choices = choices), question = true),
                entry(
                    "prompted",
                    message = Message(prompt = "Why?", links = link),
                    question = true,
                ),
                entry("alert", message = Message(choices = choices, links = link), question = true),
            )
        assertEquals(
            listOf(listOf("only-choices", "prompted"), listOf("alert")),
            listOf(inbox.waiting.map { it.exchange.id }, rows(inbox.unread)),
        )
    }

    @Test
    fun anAnsweredRequestFromTransomIsNotListed() {
        val inbox =
            inbox(
                entry("allowed", from = "@transom", state = State.REPLIED, read = true),
                entry("asking", from = "@transom", question = true),
            )
        assertEquals(
            listOf(listOf("asking"), emptyList()),
            listOf(inbox.waiting.map { it.exchange.id }, rows(inbox.earlier)),
        )
    }

    @Test
    fun oneAlertRepeatedWithItsKeyIsOneRow() {
        val keyed = Message(summary = "Disk", key = "disk")
        val inbox =
            inbox(
                entry("third", 1, keyed, read = true),
                entry("other", 2, read = true),
                entry("second", 3, keyed, read = true),
            )
        assertEquals(
            listOf(listOf("third", "other"), listOf("third", "second")),
            listOf(rows(inbox.earlier), inbox.earlier.first().ids),
        )
    }

    @Test
    fun earlierShowsTheLastWeekAndFiftyRowsAtMost() {
        val recent = (0 until 55).map { entry("recent-$it", it.toLong(), read = true) }
        val old = entry("old", 8 * 24 * 60, read = true)
        val inbox = inbox(*(recent + old).toTypedArray())
        assertEquals(
            listOf(50, 6, "old"),
            listOf(inbox.earlier.size, inbox.older.size, inbox.older.last().entry.exchange.id),
        )
    }

    @Test
    fun anInboxWithNothingListedIsEmpty() {
        assertEquals(
            listOf(true, false),
            listOf(inbox().isEmpty, inbox(entry("new")).isEmpty),
        )
    }

    @Test
    fun aRowAnswersAShortQuestionWithItsChoices() {
        val asked = entry("asked", message = Message(choices = choices), question = true)
        assertEquals(choices, asked.rowChoices("bob"))
    }

    @Test
    fun aRowLeavesToThePageWhatItCannotAnswerInOneTap() {
        val four = (1..4).map { Choice("$it") }
        val cases =
            listOf(
                entry("many", message = Message(choices = four), question = true),
                entry(
                    "prompted",
                    message = Message(choices = choices, prompt = ""),
                    question = true,
                ),
                entry("sent", message = Message(choices = choices), question = true, from = "@bob"),
                entry(
                    "answering",
                    message = Message(choices = choices),
                    sending = Sending("x", null),
                ),
                entry("over", message = Message(choices = choices), state = State.EXPIRED),
            )
        assertEquals(List(cases.size) { emptyList<Choice>() }, cases.map { it.rowChoices("bob") })
    }
}
