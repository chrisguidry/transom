package sh.transom.notifications

import android.annotation.SuppressLint
import android.app.Notification
import android.content.Context
import androidx.core.app.NotificationManagerCompat
import java.time.Instant
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import sh.transom.exchange.State
import sh.transom.exchange.Urgency
import sh.transom.sync.Client
import sh.transom.sync.Inbox
import sh.transom.sync.InboxEntry
import sh.transom.sync.Outbox
import sh.transom.sync.Store

/**
 * Keeps this phone's notifications what [store]'s copy of [name]'s inbox says they should be: one
 * for each exchange [shown] says has one, posted, updated, and cancelled as the copy changes, so a
 * notification clears when its exchange ends anywhere, is read on any device, or is gone.
 *
 * What was posted is kept in the database, so a notification posted again after the process or the
 * phone restarts makes no second sound. Android also makes none for a notification updated in
 * place, since each is posted to alert only once, except for an urgent exchange's repeat, which
 * alerts again.
 *
 * Each notification is tagged with its exchange's id, except that a message replacing an earlier
 * one with the same key, while Android still shows the earlier one, takes over its tag, so the
 * sender's alert and its resolution are one notification. A replacement of low urgency or less
 * updates it quietly.
 *
 * Android can drop a cancel of a notification the person answered from its reply box, so each
 * cancel is checked against what Android shows, and retried, as [clear] says.
 */
class Notifier(
    context: Context,
    private val store: Store,
    private val outbox: Outbox,
    private val name: String,
    client: Client,
    private val clock: () -> Instant = Instant::now,
    private val settling: Duration = 1.seconds,
) {
    private val manager = NotificationManagerCompat.from(context)
    private val posting = Posting(context, client)
    private val notified = store.database.notified()
    private val posted = mutableMapOf<String, Shown>()

    /** The tag each exchange this process posted is shown under. */
    private val tags = mutableMapOf<String, String>()
    private val answering = mutableSetOf<String>()
    private val lock = Mutex()

    /** Keeps the notifications current with the copy until cancelled. */
    suspend fun follow() = coroutineScope { store.inbox(name).collect { update(it) } }

    /**
     * Makes the notifications what the copy says now, once, as after a wake-up, and returns once
     * each one it cancelled has cleared.
     */
    suspend fun post() = coroutineScope { update(store.inbox(name).first()) }

    /**
     * Begins notifying for the session now, unless it began before. A phone that FCM wakes begins
     * when the server takes its token, so a one-way notification that arrives before the first
     * wake-up is news to it.
     */
    suspend fun begin() {
        notified.since(clock().toEpochMilli())
    }

    /**
     * Answers exchange [id] from its notification with [said]: the label of the chip the person
     * tapped when [chose], or what they typed. The answer goes through the outbox, and the
     * notification then shows it from "Me", with no actions, until the server takes it.
     *
     * Android keeps a notification answered from its reply box on screen, its spinner turning,
     * until the app posts it again. About 200 ms after the answer, Android also posts its own copy
     * of the notification as it was, which replaces what the app posted in between, and a cancel
     * that follows the app's update within a moment brings that copy back, chips and all, until the
     * cancel is retried. So the notification is left alone for [settling] after an answer, posted
     * again with the answer, and left for [settling] once more before it may clear. A cancel that
     * Android drops after that is retried, as [clear] says.
     */
    @SuppressLint("MissingPermission")
    suspend fun answer(id: String, said: String, chose: Boolean) {
        val entry = store.exchange(id).first() ?: return
        val choice = entry.exchange.message.choices.find { chose && it.label == said }
        val row = lock.withLock {
            val row = (notified.find(id) ?: Notified(id)).copy(answer = said)
            notified.put(row)
            answering += id
            row
        }
        try {
            outbox.reply(id, choice?.key, if (choice == null) said else null)
            delay(settling)
            val sent = Shown.Sent(entry, said)
            lock.withLock {
                val notification = posting.notification(sent, Alert.QUIET, clock())
                manager.notify(row.posted, ID, notification)
                posted[id] = sent
                tags[id] = row.posted
            }
            delay(settling)
        } finally {
            lock.withLock { answering -= id }
        }
        coroutineScope { update(store.inbox(name).first()) }
    }

    /**
     * The person swiped exchange [id]'s notification away, or acknowledged it, which reads it, as
     * plan 04 says. The read mark goes through the outbox, and the notification clears from the
     * copy at once: a phone that FCM wakes follows nothing in between wake-ups, and its next one
     * comes only after the server has the mark.
     */
    suspend fun dismissed(id: String) {
        outbox.mark(id, read = true)
        post()
    }

    /**
     * The person answered exchange [id] with the button of the choice [key]. The answer goes
     * through the outbox, and the notification clears from the copy at once, as [dismissed] says.
     */
    suspend fun choose(id: String, key: String) {
        outbox.reply(id, choice = key)
        post()
    }

    // Android drops a notification posted without the permission to post one, and the bell in the
    // top bar asks for it, so nothing here needs to check it first. Each notification this cancels
    // goes on clearing in the scope it runs in.
    @SuppressLint("MissingPermission")
    private suspend fun CoroutineScope.update(inbox: Inbox) = lock.withLock {
        val now = clock()
        val since = Instant.ofEpochMilli(notified.since(now.toEpochMilli()))
        val rows = notified.all().associateBy { it.exchange }
        // An exchange that a later one in the copy replaces hands its notification to that one.
        val replaced = inbox.entries.mapNotNullTo(HashSet()) { it.exchange.replaces }
        val wanted =
            inbox.entries
                .filter { it.exchange.id !in replaced }
                .mapNotNull { entry ->
                    val id = entry.exchange.id
                    entry.shown(name, since, rows[id], now)?.let { id to it }
                }
                .toMap()
        for ((id, shown) in wanted - answering) {
            val exchange = shown.entry.exchange
            val row = rows[id]
            val takeover = if (row == null) takenOver(shown.entry, rows) else null
            val tag = row?.posted ?: takeover ?: id
            val repeated = row != null && shown is Shown.Asking && exchange.repeats > row.repeats
            if (posted[id] != shown) {
                // A notification this process has posted already is updated in place, which
                // makes no sound since each alerts only once. One it has not, but which was
                // posted before, as before a restart, is posted again without one.
                val alert =
                    when {
                        repeated -> Alert.AGAIN
                        takeover != null && exchange.message.urgency <= Urgency.LOW -> Alert.QUIET
                        takeover != null -> Alert.AGAIN
                        row != null && id !in posted -> Alert.QUIET
                        exchange.created.isBefore(since) -> Alert.QUIET
                        else -> Alert.ONCE
                    }
                val notification = posting.notification(shown, alert, now)
                // Android keeps showing that an updated notification alerted a moment ago, by
                // the bell beside its time, so a quiet replacement takes the place of the one
                // it replaces rather than updating it.
                if (takeover != null && alert == Alert.QUIET) manager.cancel(tag, ID)
                manager.notify(tag, ID, notification)
                posted[id] = shown
                tags[id] = tag
            }
            if (row == null || repeated) {
                notified.put(Notified(id, row?.answer, tag.takeIf { it != id }, exchange.repeats))
            }
        }
        val held = inbox.entries.filter { it.awaitsItsReplacement(replaced) }.map { it.exchange.id }
        val showing = wanted.keys.mapNotNullTo(HashSet()) { tags[it] }
        for (id in rows.keys + posted.keys - wanted.keys - answering - held.toSet()) {
            val tag = rows[id]?.posted ?: tags.getValue(id)
            posted.remove(id)
            tags.remove(id)
            notified.drop(id)
            posting.images.forget(id)
            // A notification that a replacement took over stays, under the replacement.
            if (tag in showing) continue
            manager.cancel(tag, ID)
            launch { clear(tag) }
        }
    }

    /**
     * The tag of the notification [entry] takes over: the one of the exchange it replaces, while
     * Android still shows it. Once the person has swiped that away, the replacement is posted as a
     * notification of its own.
     */
    private fun takenOver(entry: InboxEntry, rows: Map<String, Notified>): String? {
        val earlier = rows[entry.exchange.replaces ?: return null] ?: return null
        return earlier.posted.takeIf { showing(it) != null }
    }

    /**
     * Whether [this] replaced exchange keeps its notification for now: the feed may bring an
     * exchange's `replaced` state before the message that replaced it, and the notification waits
     * for that message to take it over, rather than clearing and coming back.
     */
    private fun InboxEntry.awaitsItsReplacement(replaced: Set<String>): Boolean =
        exchange.state == State.REPLACED && exchange.id !in replaced

    /**
     * Clears the notification tagged [tag], cancelled a moment ago, if Android still shows it.
     *
     * Android drops an app's cancel of a notification the person answered from its reply box while
     * the notification Android shows is flagged for that answer. The app's update clears the flag,
     * but Android waits about 200 ms before it posts the update, and a cancel in that time is
     * dropped too. So while Android shows the notification, it is posted again as it is, without
     * the flag, and cancelled once that update has had time to post. Each try waits twice as long
     * as the last, for about 7.5 seconds in all. A tag posted again in the meantime is left alone.
     */
    @SuppressLint("MissingPermission")
    private suspend fun clear(tag: String) {
        var wait = RETRY
        repeat(RETRIES) {
            delay(wait)
            lock.withLock {
                if (tag in tags.values) return
                val shown = showing(tag) ?: return
                shown.flags = shown.flags and LIFETIME_EXTENDED.inv()
                manager.notify(tag, ID, shown)
            }
            delay(wait)
            lock.withLock { if (tag !in tags.values) manager.cancel(tag, ID) }
            wait *= 2
        }
    }

    /** The notification Android shows under [tag], if it shows one. */
    private fun showing(tag: String): Notification? =
        manager.activeNotifications.find { it.tag == tag && it.id == ID }?.notification

    companion object {
        /**
         * The id every notification of an exchange is posted with, under the exchange's id, or the
         * id of the exchange it replaced, as its tag.
         */
        const val ID = 1

        private val RETRY = 250.milliseconds
        private const val RETRIES = 4

        // Notification's FLAG_LIFETIME_EXTENDED_BY_DIRECT_REPLY, which the SDK hides. Android sets
        // it on the notification it shows, so a copy posted again leaves it out.
        private const val LIFETIME_EXTENDED = 0x00010000
    }
}
