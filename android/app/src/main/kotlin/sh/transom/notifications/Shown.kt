package sh.transom.notifications

import java.time.Instant
import sh.transom.exchange.Reply
import sh.transom.exchange.State
import sh.transom.sync.InboxEntry
import sh.transom.sync.Sending

/** What the notification of one exchange shows, if it has one. */
sealed interface Shown {
    val entry: InboxEntry

    /** The message, with Reply, Open, and its first link: an exchange waiting on this person. */
    data class Asking(override val entry: InboxEntry) : Shown

    /**
     * The message and the answer this person gave from the notification, [said], from "Me", with no
     * actions, while the outbox sends it.
     */
    data class Sent(override val entry: InboxEntry, val said: String) : Shown

    /**
     * The message, and that [mine], this person's answer, was not used, since [reply] came first.
     */
    data class Lost(override val entry: InboxEntry, val mine: Sending, val reply: Reply) : Shown
}

/**
 * What [this] entry's notification shows for [name] [now], or `null` when it has none.
 *
 * A notification asks while the exchange is open and unread by a recipient other than its sender:
 * every such question, and each one-way notification that arrived [since] notifying began or was
 * [posted] before. Once the person answers from the notification, it shows that answer until the
 * server settles it. When the server did not use an answer this phone sent, from the notification
 * or from the app, the notification says so, and who answered first, until the exchange is read.
 */
internal fun InboxEntry.shown(
    name: String,
    since: Instant,
    posted: Notified?,
    now: Instant,
): Shown? {
    if (read == null || exchange.from == "@$name") return null
    val said = posted?.answer
    if (said != null && sending != null) return Shown.Sent(this, said)
    val reply = exchange.reply
    if (lost != null && reply != null && read == false) return Shown.Lost(this, lost, reply)
    if (said != null) return null
    val open =
        exchange.state == State.OPEN &&
            sending == null &&
            read == false &&
            exchange.expires?.isAfter(now) != false
    val news = exchange.question || posted != null || !exchange.created.isBefore(since)
    return if (open && news) Shown.Asking(this) else null
}
