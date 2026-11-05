package sh.transom.notifications

import java.time.Instant
import sh.transom.exchange.Reply
import sh.transom.exchange.State
import sh.transom.sync.InboxEntry

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

    /** The message and the [reply] someone else gave before this person's answer arrived. */
    data class Answered(override val entry: InboxEntry, val reply: Reply) : Shown
}

/**
 * What [this] entry's notification shows for [name] [now], or `null` when it has none.
 *
 * A notification asks while the exchange is open and unread by a recipient other than its sender:
 * every such question, and each one-way notification that arrived [since] notifying began or was
 * [posted] before. Once the person answers from the notification, it shows that answer until the
 * server settles it, and then who answered first if it was someone else, until that is read.
 */
internal fun InboxEntry.shown(
    name: String,
    since: Instant,
    posted: Notified?,
    now: Instant,
): Shown? {
    if (read == null || exchange.from == "@$name") return null
    val said = posted?.answer
    val reply = exchange.reply
    if (said != null) {
        return when {
            sending != null -> Shown.Sent(this, said)
            reply != null && reply.by != "@$name" && read == false -> Shown.Answered(this, reply)
            else -> null
        }
    }
    val open =
        exchange.state == State.OPEN &&
            sending == null &&
            read == false &&
            exchange.expires?.isAfter(now) != false
    val news = exchange.question || posted != null || !exchange.created.isBefore(since)
    return if (open && news) Shown.Asking(this) else null
}
