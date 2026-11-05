package sh.transom.notifications

import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import sh.transom.exchange.Reply
import sh.transom.exchange.State
import sh.transom.sync.InboxEntry
import sh.transom.sync.Sending
import sh.transom.sync.exchange

/** Which of @bob's exchanges have a notification, and what it shows. */
class ShownTest {
    private val since = Instant.ofEpochSecond(100)
    private val now = Instant.ofEpochSecond(200)

    private fun InboxEntry.shown(posted: Notified? = null) = shown("bob", since, posted, now)

    private fun entry(
        question: Boolean = true,
        state: State = State.OPEN,
        read: Boolean? = false,
        from: String = "@alice",
        created: Long = 150,
        sending: Sending? = null,
        reply: Reply? = null,
        expires: Long? = null,
    ) =
        InboxEntry(
            exchange("x", from = from, state = state, question = question, created = created)
                .copy(reply = reply, expires = expires?.let(Instant::ofEpochSecond)),
            read,
            sending,
        )

    private val alices = Reply(choice = "Hold", by = "@alice", at = now)

    @Test
    fun anOpenUnreadQuestionAsks() {
        val entry = entry()
        assertEquals(Shown.Asking(entry), entry.shown())
    }

    @Test
    fun aQuestionThatArrivedBeforeNotifyingBeganStillAsks() {
        val entry = entry(created = 50)
        assertEquals(Shown.Asking(entry), entry.shown())
    }

    @Test fun aReadQuestionDoesNot() = assertNull(entry(read = true).shown())

    @Test fun anEndedQuestionDoesNot() = assertNull(entry(state = State.WITHDRAWN).shown())

    @Test fun aPendingQuestionDoesNot() = assertNull(entry(state = State.PENDING).shown())

    @Test fun aQuestionPastItsDeadlineDoesNot() = assertNull(entry(expires = 199).shown())

    @Test
    fun aQuestionBeforeItsDeadlineAsks() {
        val entry = entry(expires = 201)
        assertEquals(Shown.Asking(entry), entry.shown())
    }

    @Test fun whatBobSentDoesNot() = assertNull(entry(from = "@bob").shown())

    @Test fun whatBobIsNoRecipientOfDoesNot() = assertNull(entry(read = null).shown())

    @Test
    fun aQuestionAnsweredInTheAppDoesNot() =
        assertNull(entry(sending = Sending("Hold", null)).shown())

    @Test
    fun aOneWayNotificationThatArrivedSinceNotifyingBeganAsks() {
        val entry = entry(question = false)
        assertEquals(Shown.Asking(entry), entry.shown())
    }

    @Test
    fun aOneWayNotificationFromBeforeDoesNot() =
        assertNull(entry(question = false, created = 50).shown())

    @Test
    fun aOneWayNotificationPostedBeforeStillAsks() {
        val entry = entry(question = false, created = 50)
        assertEquals(Shown.Asking(entry), entry.shown(Notified("x")))
    }

    @Test
    fun anAnswerFromTheNotificationShowsWhileItIsSent() {
        val entry = entry(sending = Sending("hold", null), read = true)
        assertEquals(Shown.Sent(entry, "Hold"), entry.shown(Notified("x", answer = "Hold")))
    }

    @Test
    fun anAnswerThatLostShowsWhoAnsweredFirst() {
        val entry = entry(state = State.REPLIED, reply = alices)
        assertEquals(
            Shown.Answered(entry, alices),
            entry.shown(Notified("x", answer = "Deploy")),
        )
    }

    @Test
    fun whoAnsweredFirstClearsOnceRead() =
        assertNull(
            entry(state = State.REPLIED, reply = alices, read = true)
                .shown(Notified("x", answer = "Deploy"))
        )

    @Test
    fun anAnswerTheServerTookClears() =
        assertNull(
            entry(state = State.REPLIED, reply = alices.copy(by = "@bob"), read = true)
                .shown(Notified("x", answer = "Hold"))
        )
}
