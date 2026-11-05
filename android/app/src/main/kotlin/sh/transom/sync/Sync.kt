package sh.transom.sync

import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Keeps [store]'s copy of [name]'s exchanges what the server says they are, with [client].
 *
 * ```
 * val sync = Sync(Client(session.server, session.token), store, session.name)
 * scope.launch { sync.follow() }   // while the inbox is on screen, or in the foreground service
 * store.inbox(session.name).collect { inbox -> show(inbox.waiting, inbox.unread, inbox.earlier) }
 * ```
 */
class Sync(
    private val client: Client,
    private val store: Store,
    private val name: String,
    private val backoff: Backoff = Backoff(),
    private val heard: () -> Unit = {},
) {
    private val copies = store.database.copies()

    private val following = MutableStateFlow(false)

    /** Whether the live feed is connected now, so the phone hears of each change as it happens. */
    val connected: StateFlow<Boolean> = following.asStateFlow()

    /**
     * Copies the server's whole list, and drops what it no longer lists: an exchange of a group
     * this person left, or one the server deleted for its age.
     */
    suspend fun refresh() {
        copies.replace(client.exchanges(name))
    }

    /**
     * Follows the live feed until cancelled, reconnecting after [backoff] each time the connection
     * fails or closes. Each connection refreshes from the list once it opens, since the feed sends
     * only what changes after that, and the feed's events wait until the refresh is copied, so an
     * event never loses to a list that was read before it.
     */
    suspend fun follow(): Nothing {
        var failures = 0
        while (true) {
            try {
                client.feed(name).collect { event ->
                    when (event) {
                        FeedEvent.Opened -> refresh().also { failures = 0 }
                        is FeedEvent.Changed -> copies.save(event.exchange, name)
                        is FeedEvent.Gone -> copies.drop(event.id)
                    }
                    following.value = true
                    heard()
                }
            } catch (_: Exception) {
                // Cancelling ends following, and any other failure is a lost connection.
                currentCoroutineContext().ensureActive()
            } finally {
                following.value = false
            }
            delay(backoff.after(failures++))
        }
    }

    /** Withdraws exchange [id], which [name] sent, and copies the server's answer. */
    suspend fun withdraw(id: String) {
        copies.save(client.withdraw(id), name)
    }

    /**
     * Registers this phone as one of [name]'s devices, called [deviceName], which the server wakes
     * by [wake]. The phone's device id is made the first time and kept, so each registration
     * replaces the one before.
     */
    suspend fun register(deviceName: String, wake: Wake = Wake.Connection) {
        client.register(name, store.database.device().id(), deviceName, wake)
    }
}
