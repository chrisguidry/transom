package sh.transom

import android.content.Context
import android.os.Build
import androidx.work.WorkManager
import java.io.IOException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.job
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import sh.transom.notifications.Notifier
import sh.transom.signin.Session
import sh.transom.sync.Backoff
import sh.transom.sync.Client
import sh.transom.sync.Outbox
import sh.transom.sync.OutboxWorker
import sh.transom.sync.Refused
import sh.transom.sync.Store
import sh.transom.sync.Sync
import sh.transom.sync.SyncDatabase

/**
 * What the app does as the one [session] it is signed in with: the [client] that speaks for it, the
 * [store] that keeps its copy of the inbox, the [sync] that keeps the copy current, the [outbox]
 * that WorkManager sends, and the [notifier] that posts its notifications. The app builds one for
 * each sign-in, and [forget] ends it.
 *
 * The feed, the notifier, the outbox's deliveries, and the device's registration run in a scope of
 * the account's own, so [forget] can stop them all before it clears the copy, and none of them
 * writes to the copy after it is cleared.
 */
class Account(
    val session: Session,
    context: Context,
    private val work: WorkManager,
    http: OkHttpClient,
    database: SyncDatabase,
    private val backoff: Backoff = Backoff(),
) {
    val client = Client(session.server, session.token, http)
    val store = Store(database)
    val sync = Sync(client, store, session.name, backoff)
    val outbox = Outbox(store) { OutboxWorker.schedule(work) }
    val notifier = Notifier(context, store, outbox, session.name, client::resolve)

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    /** How many callers of [follow] are following now. */
    private val followers = MutableStateFlow(0)

    private var registering: Job? = null

    init {
        // One feed, however many follow it: the inbox on screen and the background connection
        // both do, and a second connection would only copy the same events twice.
        scope.launch {
            followers
                .map { it > 0 }
                .distinctUntilChanged()
                .collectLatest { following -> if (following) sync.follow() }
        }
    }

    /** The `@name` the session signs in as, without its `@`. */
    val name: String
        get() = session.name

    /**
     * Follows the live feed until the caller is cancelled, as the inbox does while the app is on
     * screen, or until the account is forgotten.
     */
    suspend fun follow() {
        followers.update { it + 1 }
        try {
            scope.coroutineContext.job.join()
        } finally {
            followers.update { it - 1 }
        }
    }

    /** Keeps the notifications current, as the background connection does, until cancelled. */
    suspend fun notifying() {
        background { notifier.follow() }.join()
    }

    /**
     * Runs [work] as this account, such as an answer from a notification, so forgetting the account
     * stops it.
     */
    fun background(work: suspend () -> Unit): Job = scope.launch { work() }

    /** Sends the outbox, as [OutboxWorker] asks, and returns `false` to be asked again later. */
    suspend fun deliver(): Boolean = scope.async { outbox.deliver(client, name) }.await()

    /**
     * Registers this phone as a device of the session's, which wakes by holding the live feed open,
     * once per account however often it is asked. It tries again after [backoff] while the server
     * cannot be reached or fails for now, and gives up on a refusal that would come again.
     */
    @Synchronized
    fun register(): Job =
        registering
            ?: scope
                .launch {
                    var failures = 0
                    while (!registered()) delay(backoff.after(failures++))
                }
                .also { registering = it }

    /** Whether the registration is done with, taken or refused for good. */
    private suspend fun registered(): Boolean =
        try {
            sync.register(Build.MODEL)
            true
        } catch (_: IOException) {
            false
        } catch (refused: Refused) {
            !refused.transient
        }

    /**
     * Withdraws exchange [id], which this person sent, and returns `false` when the server could
     * not be reached or refused, such as for a question answered a moment before.
     */
    suspend fun withdraw(id: String): Boolean =
        try {
            sync.withdraw(id)
            true
        } catch (_: IOException) {
            false
        } catch (_: Refused) {
            false
        }

    /**
     * Ends the account, as signing out does: stops what it runs, drops what the outbox would still
     * send, and clears the copy, so the next person to sign in on this phone sees none of it.
     */
    suspend fun forget() {
        scope.coroutineContext.job.cancelAndJoin()
        work.cancelUniqueWork(OutboxWorker.NAME)
        store.clear()
    }
}
