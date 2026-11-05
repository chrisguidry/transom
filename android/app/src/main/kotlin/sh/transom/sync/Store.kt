package sh.transom.sync

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
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
 * [name]'s inbox, split as the web inbox splits it: [waiting] holds the open questions someone else
 * asked, [unread] holds the rest of what this person has not read, and [others] holds everything
 * else, each newest first.
 *
 * [entries] is every exchange in the copy, and the sections draw on [listed], which leaves out each
 * exchange that a later one with the same key replaced, so a firing alert and its resolution are
 * one row. An exchange that the copy has a replacement of is left out even before its own
 * `replaced` state arrives, since the feed may bring the two in either order.
 */
data class Inbox(val name: String, val entries: List<InboxEntry>) {
    private val replaced = entries.mapNotNullTo(HashSet()) { it.exchange.replaces }

    private fun InboxEntry.isReplaced() =
        exchange.state == State.REPLACED || exchange.id in replaced

    val listed: List<InboxEntry> = entries.filterNot { it.isReplaced() }
    val waiting: List<InboxEntry> = listed.filter { it.isWaitingOn(name) }
    val unread: List<InboxEntry> = listed.filter { !it.isWaitingOn(name) && it.read == false }
    val others: List<InboxEntry> = listed.filter { !it.isWaitingOn(name) && it.read != false }

    /**
     * What Mark all read marks: each exchange in [unread], and each replaced one this person never
     * read. The inbox no longer lists those, and the web's lists them as unread until they are.
     */
    val markable: List<String> =
        (unread + entries.filter { it.isReplaced() && it.read == false }).map { it.exchange.id }
}

/**
 * One exchange in the inbox. [read] is `null` when this person is not one of its recipients.
 * [sending] is a reply this phone has queued and the server has not answered yet, which the
 * exchange shows in place of its own until the server settles it.
 */
data class InboxEntry(val exchange: Exchange, val read: Boolean?, val sending: Sending?) {
    /**
     * Whether this is an open question someone other than [name] asked, which [name] has not
     * answered.
     */
    fun isWaitingOn(name: String): Boolean =
        exchange.question &&
            exchange.state == State.OPEN &&
            exchange.from != "@$name" &&
            sending == null
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
