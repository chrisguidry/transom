package sh.transom

import android.content.Context
import android.os.Build
import androidx.work.WorkManager
import java.io.File
import java.io.IOException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.job
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import sh.transom.delivery.DeliveryProblem
import sh.transom.delivery.DeliveryState
import sh.transom.delivery.Push
import sh.transom.notifications.Notifier
import sh.transom.notifications.PICTURES
import sh.transom.pictures.Pictures
import sh.transom.signin.Session
import sh.transom.sync.Backoff
import sh.transom.sync.Client
import sh.transom.sync.Outbox
import sh.transom.sync.OutboxWorker
import sh.transom.sync.Refused
import sh.transom.sync.Store
import sh.transom.sync.Sync
import sh.transom.sync.SyncDatabase
import sh.transom.sync.Wake

/**
 * What the app does as the one [session] it is signed in with: the [client] that speaks for it, the
 * [store] that keeps its copy of the inbox, the [sync] that keeps the copy current, the [outbox]
 * that WorkManager sends, and the [notifier] that posts its notifications. The app builds one for
 * each sign-in, and [forget] ends it.
 *
 * The feed, the notifier, the outbox's deliveries, and the device's registration run in a scope of
 * the account's own, so [forget] can stop them all before it clears the copy, and none of them
 * writes to the copy after it is cleared.
 *
 * Notifications reach the phone through [push], FCM, when the person chose Google Play services,
 * and otherwise through the connection the phone holds open. [delivery] says where that stands.
 */
class Account(
    val session: Session,
    context: Context,
    private val work: WorkManager,
    http: OkHttpClient,
    database: SyncDatabase,
    private val backoff: Backoff = Backoff(),
    private var push: Push? = null,
) {
    val client = Client(session.server, session.token, http)
    val store = Store(database)
    val sync = Sync(client, store, session.name, backoff)
    val outbox = Outbox(store) { OutboxWorker.schedule(work) }
    val notifier = Notifier(context, store, outbox, session.name, client)

    /** The pictures of the people and agents in the inbox, kept in the app's cache. */
    val pictures = Pictures(client, File(context.cacheDir, PICTURES))
    private val problem = DeliveryProblem(context)

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    /** How many callers of [follow] are following now. */
    private val followers = MutableStateFlow(0)

    private var registering: Job? = null

    private val delivering = MutableStateFlow(stateOf(push))

    /**
     * Where delivery stands: [DeliveryState.Connection] without [push], and with it,
     * [DeliveryState.Registering] until the registration settles, then [DeliveryState.Delivering]
     * once the server takes the token, or [DeliveryState.Failing] with the reason it doesn't.
     */
    val delivery: StateFlow<DeliveryState> = delivering.asStateFlow()

    init {
        // The delivery problem's notification follows the state, for as long as the account runs.
        scope.launch { delivering.collect(problem::show) }
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
     * Refreshes the copy from the server once and makes the notifications what it says, as a
     * wake-up does, and returns `false` when the server could not be reached, to be asked again
     * later.
     */
    suspend fun catchUp(): Boolean =
        scope
            .async {
                try {
                    sync.refresh()
                } catch (_: IOException) {
                    return@async false
                }
                notifier.post()
                true
            }
            .await()

    /**
     * Runs [work] as this account, such as an answer from a notification, so forgetting the account
     * stops it.
     */
    fun background(work: suspend () -> Unit): Job = scope.launch { work() }

    /** Sends the outbox, as [OutboxWorker] asks, and returns `false` to be asked again later. */
    suspend fun deliver(): Boolean = scope.async { outbox.deliver(client, name) }.await()

    /**
     * Registers this phone as a device of the session's, once per account however often it is
     * asked: with FCM's token when there is a [push], and otherwise as a phone that holds the live
     * feed open. Once it settles, [delivery] says how it went.
     */
    @Synchronized fun register(): Job = registering ?: registration()

    /**
     * Delivers through [push] from here on, or through the connection when it is `null`, as when
     * the person changes the choice in Settings: the phone registers again under the same device
     * id, in place of a registration still trying.
     */
    @Synchronized
    fun deliverBy(push: Push?): Job {
        registering?.cancel()
        this.push = push
        delivering.value = stateOf(push)
        return registration()
    }

    /**
     * Registers FCM's new [token] in place of the one before, and of a registration still trying,
     * while the phone delivers through Google Play services. With the connection chosen, the token
     * is not the server's business, and this does nothing.
     */
    @Synchronized
    fun registerToken(token: String): Job? {
        if (push == null) return null
        registering?.cancel()
        return scope.launch { settle(token) }.also { registering = it }
    }

    private fun registration(): Job =
        scope
            .launch {
                val push = push
                if (push == null) registered(Wake.Connection) else settle(push.token())
            }
            .also { registering = it }

    private suspend fun settle(token: String?) {
        delivering.value =
            when {
                token == null -> DeliveryState.Failing.NoToken
                // A server that does not take FCM devices refuses the token with 422
                // Unprocessable Entity, a refusal that would come again.
                !registered(Wake.Fcm(token)) -> DeliveryState.Failing.Refused
                else -> {
                    notifier.begin()
                    DeliveryState.Delivering(token)
                }
            }
    }

    /**
     * Registers this phone as a device that the server wakes by [wake], and returns whether the
     * server took it. It tries again after [backoff] while the server cannot be reached or fails
     * for now, and gives up on a refusal that would come again.
     */
    private suspend fun registered(wake: Wake): Boolean {
        var failures = 0
        while (true) {
            try {
                sync.register(Build.MODEL, wake)
                return true
            } catch (_: IOException) {} catch (refused: Refused) {
                if (!refused.transient) return false
            }
            delay(backoff.after(failures++))
        }
    }

    private fun stateOf(push: Push?): DeliveryState =
        if (push == null) DeliveryState.Connection else DeliveryState.Registering

    /** The id this phone registers as a device under. */
    suspend fun deviceId(): String = store.database.device().id()

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
     * Stops what the account runs and leaves its copy and queue for the account that follows it, as
     * after a rename, when the same session goes on under its new name.
     */
    fun close() {
        scope.cancel()
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
