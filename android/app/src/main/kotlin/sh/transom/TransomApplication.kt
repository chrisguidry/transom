package sh.transom

import android.app.Application
import androidx.core.app.NotificationManagerCompat
import androidx.work.Configuration
import androidx.work.WorkManager
import javax.crypto.SecretKey
import okhttp3.OkHttpClient
import sh.transom.notifications.Channels
import sh.transom.notifications.ConnectionService
import sh.transom.settings.Preferences
import sh.transom.signin.AndroidKeystoreOnly
import sh.transom.signin.SessionStore
import sh.transom.signin.SignInRequests
import sh.transom.signin.androidKeystoreKey
import sh.transom.sync.OutboxWorkerFactory
import sh.transom.sync.SyncDatabase

/**
 * The app's process: the one session store, HTTP client, and database every screen shares, and the
 * [Account] of the session it is signed in with. It is open so the unit tests can seal the session
 * with a key of their own, since Robolectric has no Keystore.
 *
 * It gives WorkManager its configuration, in place of the default initializer the manifest removes,
 * so the outbox's worker is made with the account that sends it. WorkManager may start the process
 * for that work alone, with no screen open, so the account is built from the stored session
 * whenever it is first asked for.
 */
open class TransomApplication : Application(), Configuration.Provider {
    val sessions: SessionStore by lazy { SessionStore(noBackupFilesDir, ::sessionKey) }

    val signInRequests: SignInRequests by lazy { SignInRequests(http) }

    val database: SyncDatabase by lazy { SyncDatabase.open(this) }

    val preferences: Preferences by lazy { Preferences(this) }

    private val http: OkHttpClient by lazy { OkHttpClient() }

    private var account: Account? = null

    /** The account of the session the app is signed in with, or `null` when it is signed out. */
    @Synchronized
    fun account(): Account? =
        account
            ?: sessions.session()?.let { session ->
                Account(session, this, WorkManager.getInstance(this), http, database).also {
                    account = it
                }
            }

    override fun onCreate() {
        super.onCreate()
        Channels.create(this)
    }

    /**
     * Begins what the app does once it is signed in: registering the phone as a device, and the
     * background connection, which follows the feed and posts the notifications, unless the person
     * turned it off. Each begins once however often this is called.
     */
    fun signedIn() {
        account()?.register()
        if (preferences.backgroundConnection) ConnectionService.start(this)
    }

    /**
     * Begins again as the stored session, as after a rename: the account of the session as it was
     * stops, and the next is built from the session as it is now. The copy of the inbox stays, and
     * the new account's first refresh brings it the new name.
     */
    fun restarted() {
        val ended = synchronized(this) { account.also { account = null } }
        ended?.close()
        signedIn()
    }

    /**
     * Forgets the account, as signing out does, once the session itself is forgotten: stops the
     * background connection and clears every notification the account posted.
     */
    suspend fun signedOut() {
        val ended = synchronized(this) { account.also { account = null } }
        ConnectionService.stop(this)
        ended?.forget()
        NotificationManagerCompat.from(this).cancelAll()
    }

    override val workManagerConfiguration: Configuration
        get() =
            Configuration.Builder()
                .setWorkerFactory(OutboxWorkerFactory { account()?.deliver() ?: true })
                .build()

    @AndroidKeystoreOnly protected open fun sessionKey(): SecretKey = androidKeystoreKey()
}
