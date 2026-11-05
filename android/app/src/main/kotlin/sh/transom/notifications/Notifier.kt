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
import sh.transom.sync.Inbox
import sh.transom.sync.Outbox
import sh.transom.sync.Store

/**
 * Keeps this phone's notifications what [store]'s copy of [name]'s inbox says they should be: one
 * for each exchange [shown] says has one, posted, updated, and cancelled as the copy changes, so a
 * notification clears when its exchange ends anywhere, is read on any device, or is gone.
 *
 * What was posted is kept in the database, so a notification posted again after the process or the
 * phone restarts makes no second sound. Android also makes none for a notification updated in
 * place, since each is posted to alert only once.
 *
 * Android can drop a cancel of a notification the person answered from its reply box, so each
 * cancel is checked against what Android shows, and retried, as [clear] says.
 */
class Notifier(
    context: Context,
    private val store: Store,
    private val outbox: Outbox,
    private val name: String,
    resolve: (String) -> String,
    private val clock: () -> Instant = Instant::now,
    private val settling: Duration = 1.seconds,
) {
    private val manager = NotificationManagerCompat.from(context)
    private val posting = Posting(context, resolve)
    private val notified = store.database.notified()
    private val posted = mutableMapOf<String, Shown>()
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
        lock.withLock {
            notified.put(Notified(id, answer = said))
            answering += id
        }
        try {
            outbox.reply(id, choice?.key, if (choice == null) said else null)
            delay(settling)
            val sent = Shown.Sent(entry, said)
            lock.withLock {
                manager.notify(id, ID, posting.notification(sent, silent = true, clock()))
                posted[id] = sent
            }
            delay(settling)
        } finally {
            lock.withLock { answering -= id }
        }
        coroutineScope { update(store.inbox(name).first()) }
    }

    /** The person swiped exchange [id]'s notification away, which reads it, as plan 04 says. */
    suspend fun dismissed(id: String) {
        outbox.mark(id, read = true)
    }

    // Android drops a notification posted without the permission to post one, and the bell in the
    // top bar asks for it, so nothing here needs to check it first. Each notification this cancels
    // goes on clearing in the scope it runs in.
    @SuppressLint("MissingPermission")
    private suspend fun CoroutineScope.update(inbox: Inbox) = lock.withLock {
        val now = clock()
        val since = Instant.ofEpochMilli(notified.since(now.toEpochMilli()))
        val rows = notified.all().associateBy { it.exchange }
        val wanted =
            inbox.entries
                .mapNotNull { entry ->
                    val id = entry.exchange.id
                    entry.shown(name, since, rows[id], now)?.let { id to it }
                }
                .toMap()
        for ((id, shown) in wanted - answering) {
            if (posted[id] != shown) {
                // A notification this process has posted already is updated in place, which
                // makes no sound since each alerts only once. One it has not, but which was
                // posted before, as before a restart, is posted again without one.
                val known =
                    (id in rows && id !in posted) || shown.entry.exchange.created.isBefore(since)
                manager.notify(id, ID, posting.notification(shown, silent = known, now))
                posted[id] = shown
            }
            if (id !in rows) notified.put(Notified(id))
        }
        for (id in rows.keys + posted.keys - wanted.keys - answering) {
            manager.cancel(id, ID)
            posted.remove(id)
            notified.drop(id)
            launch { clear(id) }
        }
    }

    /**
     * Clears exchange [id]'s notification, cancelled a moment ago, if Android still shows it.
     *
     * Android drops an app's cancel of a notification the person answered from its reply box while
     * the notification Android shows is flagged for that answer. The app's update clears the flag,
     * but Android waits about 200 ms before it posts the update, and a cancel in that time is
     * dropped too. So while Android shows the notification, it is posted again as it is, without
     * the flag, and cancelled once that update has had time to post. Each try waits twice as long
     * as the last, for about 7.5 seconds in all. An exchange posted again in the meantime is left
     * alone.
     */
    @SuppressLint("MissingPermission")
    private suspend fun clear(id: String) {
        var wait = RETRY
        repeat(RETRIES) {
            delay(wait)
            lock.withLock {
                if (id in posted) return
                val shown = showing(id) ?: return
                shown.flags = shown.flags and LIFETIME_EXTENDED.inv()
                manager.notify(id, ID, shown)
            }
            delay(wait)
            lock.withLock { if (id !in posted) manager.cancel(id, ID) }
            wait *= 2
        }
    }

    /** The notification Android shows for exchange [id], if it shows one. */
    private fun showing(id: String): Notification? =
        manager.activeNotifications.find { it.tag == id && it.id == ID }?.notification

    companion object {
        /**
         * The id every notification of an exchange is posted with, under the exchange's id as its
         * tag.
         */
        const val ID = 1

        private val RETRY = 250.milliseconds
        private const val RETRIES = 4

        // Notification's FLAG_LIFETIME_EXTENDED_BY_DIRECT_REPLY, which the SDK hides. Android sets
        // it on the notification it shows, so a copy posted again leaves it out.
        private const val LIFETIME_EXTENDED = 0x00010000
    }
}
