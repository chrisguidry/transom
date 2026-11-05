package sh.transom

import android.os.Build
import androidx.work.WorkManager
import java.io.IOException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.job
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import sh.transom.signin.Session
import sh.transom.sync.Client
import sh.transom.sync.Outbox
import sh.transom.sync.OutboxWorker
import sh.transom.sync.Refused
import sh.transom.sync.Store
import sh.transom.sync.Sync
import sh.transom.sync.SyncDatabase

/**
 * What the app does as the one [session] it is signed in with: the [client] that speaks for it, the
 * [store] that keeps its copy of the inbox, the [sync] that keeps the copy current, and the
 * [outbox] that WorkManager sends. The app builds one for each sign-in, and [forget] ends it.
 *
 * The feed, the outbox's deliveries, and the device's registration run in a scope of the account's
 * own, so [forget] can stop them all before it clears the copy, and none of them writes to the copy
 * after it is cleared.
 */
class Account(
    val session: Session,
    private val work: WorkManager,
    http: OkHttpClient,
    database: SyncDatabase,
) {
    val client = Client(session.server, session.token, http)
    val store = Store(database)
    val sync = Sync(client, store, session.name)
    val outbox = Outbox(store) { OutboxWorker.schedule(work) }

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    /** The `@name` the session signs in as, without its `@`. */
    val name: String
        get() = session.name

    /**
     * Follows the live feed until the caller is cancelled, as the inbox does while the app is on
     * screen, or until the account is forgotten.
     */
    suspend fun follow() {
        val following = scope.launch { sync.follow() }
        try {
            following.join()
        } finally {
            following.cancel()
        }
    }

    /** Sends the outbox, as [OutboxWorker] asks, and returns `false` to be asked again later. */
    suspend fun deliver(): Boolean = scope.async { outbox.deliver(client, name) }.await()

    /**
     * Registers this phone as a device of the session's, which wakes by holding the live feed open.
     * A registration that fails is not tried again until the next sign-in: the server pushes
     * nothing to a device of this kind, so until the background connection exists, it only names
     * the phone in the person's list of devices.
     */
    fun register() {
        scope.launch { runCatching { sync.register(Build.MODEL) } }
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
