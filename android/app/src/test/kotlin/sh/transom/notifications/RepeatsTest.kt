package sh.transom.notifications

import android.app.Notification
import androidx.core.app.NotificationManagerCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.exchange.Message
import sh.transom.exchange.Urgency
import sh.transom.signin.app
import sh.transom.sync.Client
import sh.transom.sync.MemoryStore
import sh.transom.sync.Outbox
import sh.transom.sync.exchange
import sh.transom.testserver.NOWHERE

/**
 * An urgent exchange's repeats, which the server counts on the exchange, alert again while it is
 * unread. The server repeats only minutes apart, so the copy is given each count as the feed would
 * bring it.
 */
@RunWith(RobolectricTestRunner::class)
class RepeatsTest {
    @get:Rule val memory = MemoryStore()

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    // After notifying begins, so the one-way page is news.
    private val arrived = System.currentTimeMillis() / 1000 + 60

    @After fun stop() = scope.cancel()

    private fun notifying() {
        val store = memory.store
        val notifier = Notifier(app, store, Outbox(store) {}, "bob", Client(NOWHERE, "token"))
        scope.launch { notifier.follow() }
    }

    private fun arrives(repeats: Int, read: Boolean = false) = runBlocking {
        val urgent = Message(summary = "UPS on battery", urgency = Urgency.URGENT)
        val page =
            exchange("page", question = false, read = read, created = arrived)
                .copy(message = urgent)
        memory.store.database.copies().replace(listOf(page.copy(repeats = repeats)))
    }

    private val Notification.alertsOnce: Boolean
        get() = flags and Notification.FLAG_ONLY_ALERT_ONCE != 0

    @Test
    fun aRepeatAlertsAgain() {
        notifying()
        arrives(repeats = 0)
        awaitPosted("page")
        arrives(repeats = 1)
        val repeated = awaitPosted("page") { !it.alertsOnce }
        assertEquals(false, repeated.silent)
    }

    @Test
    fun aRepeatHeardBeforeARestartIsQuiet() {
        notifying()
        arrives(repeats = 1)
        awaitPosted("page")
        // A restarted phone shows none of the notifications it had.
        NotificationManagerCompat.from(app).cancelAll()
        notifying()
        assertEquals(true, awaitPosted("page").silent)
    }

    @Test
    fun aRepeatOfOneReadIsNotShown() {
        notifying()
        arrives(repeats = 0)
        awaitPosted("page")
        arrives(repeats = 1, read = true)
        awaitCleared("page")
    }
}
