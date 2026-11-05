package sh.transom.sync

import java.time.Duration
import java.time.Instant
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import sh.transom.exchange.Choice
import sh.transom.exchange.Exchange
import sh.transom.exchange.State

/**
 * The inbox as the phone has it: the server's last description of each exchange, with what this
 * phone has queued for it applied on top, so a reply sent offline shows at once.
 */
class Store(internal val database: SyncDatabase) {
    /** [name]'s inbox, again each time the copy or the queue changes. */
    fun inbox(name: String): Flow<Inbox> =
        database.copies().rows().map { rows -> Inbox(name, rows.map(Row::entry)) }

    /** Exchange [id] as the copy has it, or `null` while it has none. */
    fun exchange(id: String): Flow<InboxEntry?> = database.copies().row(id).map { it?.entry() }

    /**
     * Forgets every exchange, everything queued, and every notification posted, as signing out
     * does. The phone's device id is kept, so the next sign-in registers the same device again
     * rather than adding another.
     */
    suspend fun clear() {
        database.notified().clear()
        database.notified().clearSince()
        database.queue().clearReplies()
        database.queue().clearReads()
        database.copies().clear()
    }
}

/**
 * [name]'s inbox at [now], sorted as the web's inbox sorts it, by the rules plan 18 records:
 * [waiting] holds the open questions that wait on this person, [unread] the rest of what they have
 * not read, and [earlier] everything else from the last week, 50 rows at most, with [older] past
 * that. Each is newest first.
 *
 * [entries] is every exchange in the copy, and the sections leave out each exchange that a later
 * one with the same key replaced, so a firing alert and its resolution are one row, and each
 * request from `@transom` that was answered, since its outcome is the sender's own message. An
 * exchange that the copy has a replacement of is left out even before its own `replaced` state
 * arrives, since the feed may bring the two in either order. In [unread] and [earlier], the
 * exchanges one sender sent with the same key are one [Listing].
 */
data class Inbox(
    val name: String,
    val entries: List<InboxEntry>,
    val now: Instant = Instant.now(),
) {
    private val replaced = entries.mapNotNullTo(HashSet()) { it.exchange.replaces }

    private fun InboxEntry.isReplaced() =
        exchange.state == State.REPLACED || exchange.id in replaced

    private fun InboxEntry.isHidden() =
        isReplaced() || (exchange.from == TRANSOM && exchange.state != State.OPEN)

    private val listed = entries.filterNot { it.isHidden() }
    val waiting: List<InboxEntry> = listed.filter { it.isWaitingOn(name) }
    val unread: List<Listing> =
        collapse(listed.filter { !it.isWaitingOn(name) && it.read == false })
    private val read = collapse(listed.filter { !it.isWaitingOn(name) && it.read != false })
    private val weekAgo = now.minus(Duration.ofDays(7))
    private val shown =
        read
            .asSequence()
            .take(EARLIER_ROWS)
            .takeWhile { !it.entry.exchange.created.isBefore(weekAgo) }
            .count()
    val earlier: List<Listing> = read.take(shown)
    val older: List<Listing> = read.drop(shown)

    /** Whether there is nothing in the inbox at all. */
    val isEmpty: Boolean
        get() = listed.isEmpty()

    /**
     * What Mark all read marks: each exchange in [unread], and each replaced one this person never
     * read, which the inbox no longer lists.
     */
    val markable: List<String> =
        unread.flatMap { it.ids } +
            entries.filter { it.isReplaced() && it.read == false }.map { it.exchange.id }
}

/** One row of the inbox: the newest of [entries], which repeat one alert under one key. */
data class Listing(val entries: List<InboxEntry>) {
    val entry: InboxEntry
        get() = entries.first()

    val ids: List<String>
        get() = entries.map { it.exchange.id }
}

/** [entries] as rows, the exchanges one sender sent one recipient with the same key as one. */
private fun collapse(entries: List<InboxEntry>): List<Listing> =
    entries
        .groupBy { entry ->
            entry.exchange.message.key?.let { Triple(entry.exchange.from, entry.exchange.to, it) }
                ?: entry.exchange.id
        }
        .values
        .map(::Listing)

private const val EARLIER_ROWS = 50

/** transom's own identity, which sends consent requests and invitations. */
private const val TRANSOM = "@transom"

/**
 * One exchange in the inbox. [read] is `null` when this person is not one of its recipients.
 * [sending] is a reply this phone has queued and the server has not answered yet, which the
 * exchange shows in place of its own until the server settles it.
 */
data class InboxEntry(val exchange: Exchange, val read: Boolean?, val sending: Sending?) {
    /**
     * Whether this waits on [name]: an open question someone else asked, which asks for an answer
     * rather than offering actions, and which [name] has not answered. A question with a prompt
     * asks for one, and so does one with only choices. One with choices and links, and no prompt,
     * is an alert whose choices are actions, such as Silence beside a dashboard: nothing breaks if
     * no one takes them. The web's `inbox::waits_on` is the same rule.
     */
    fun isWaitingOn(name: String): Boolean =
        exchange.question &&
            exchange.state == State.OPEN &&
            exchange.from != "@$name" &&
            sending == null &&
            (exchange.message.prompt != null || exchange.message.links.isEmpty())

    /**
     * The choices [name] may answer with from a row, without opening the exchange: those of an open
     * question to them, three at most, when no prompt asks for text.
     */
    fun rowChoices(name: String): List<Choice> {
        val message = exchange.message
        val answerable =
            exchange.state == State.OPEN &&
                exchange.from != "@$name" &&
                read != null &&
                sending == null &&
                message.prompt == null &&
                message.choices.size <= 3
        return if (answerable) message.choices else emptyList()
    }
}

/** A reply queued on this phone and not yet settled by the server. */
data class Sending(val choice: String?, val text: String?)

/**
 * The entry a row makes. A queued read mark is what this person last decided, so it wins over the
 * copy, and a reply reads the exchange, as it does on the server.
 */
private fun Row.entry(): InboxEntry {
    val sending = if (sending) Sending(choice, text) else null
    val read = read?.let { marked ?: (sending != null || it) }
    return InboxEntry(exchange, read, sending)
}
