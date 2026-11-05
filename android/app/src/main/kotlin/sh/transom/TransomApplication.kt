package sh.transom

import android.app.Application
import androidx.core.app.NotificationManagerCompat
import androidx.work.Configuration
import androidx.work.DelegatingWorkerFactory
import androidx.work.WorkManager
import javax.crypto.SecretKey
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
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
import sh.transom.sync.Wake
import sh.transom.wake.Firebase
import sh.transom.wake.FirebaseOnly
import sh.transom.wake.Push
import sh.transom.wake.WakeWorkerFactory

/**
 * The app's process: the one session store, HTTP client, and database every screen shares, and the
 * [Account] of the session it is signed in with. It is open so the unit tests can seal the session
 * with a key of their own, since Robolectric has no Keystore, and give it FCM, since Robolectric
 * has no Google Play services.
 *
 * It gives WorkManager its configuration, in place of the default initializer the manifest removes,
 * so the outbox's worker and the wake-up's are made with the account they work for. WorkManager may
 * start the process for that work alone, with no screen open, so the account is built from the
 * stored session whenever it is first asked for.
 */
open class TransomApplication : Application(), Configuration.Provider {
    val sessions: SessionStore by lazy { SessionStore(noBackupFilesDir, ::sessionKey) }

    val signInRequests: SignInRequests by lazy { SignInRequests(http) }

    val database: SyncDatabase by lazy { SyncDatabase.open(this) }

    val preferences: Preferences by lazy { Preferences(this) }

    private val http: OkHttpClient by lazy { OkHttpClient() }

    private val firebase by lazy { Firebase.push(this) }

    private val main = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    private var account: Account? = null

    /** The account of the session the app is signed in with, or `null` when it is signed out. */
    @Synchronized
    fun account(): Account? =
        account
            ?: sessions.session()?.let { session ->
                val work = WorkManager.getInstance(this)
                Account(session, this, work, http, database, push = push()).also { account = it }
            }

    override fun onCreate() {
        super.onCreate()
        Channels.create(this)
        // Firebase starts with the process, before FCM hands the app a wake-up or a token.
        push()
    }

    /**
     * Begins what the app does once it is signed in: registering the phone as a device, and, unless
     * FCM wakes the phone, the background connection, which follows the feed and posts the
     * notifications, unless the person turned it off. Each begins once however often this is
     * called.
     */
    fun signedIn() {
        val account = account() ?: return
        account.register()
        connectUnlessPushed(account)
    }

    /**
     * Registers FCM's new [token] for the account signed in, and holds the connection instead if
     * the server refuses it.
     */
    fun newToken(token: String) {
        val account = account() ?: return
        val registering = account.registerToken(token)
        main.launch {
            registering.join()
            connectUnlessPushed(account)
        }
    }

    /**
     * Starts the background connection once [account]'s registration settles, unless FCM wakes the
     * phone or the person turned the connection off.
     */
    private fun connectUnlessPushed(account: Account) {
        main.launch {
            if (account.wake() == Wake.Connection && preferences.backgroundConnection) {
                // The registration may settle after the app has left the screen, when Android
                // refuses to start a foreground service, and the app coming to the front starts
                // it then.
                runCatching { ConnectionService.start(this@TransomApplication) }
            }
        }
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
        get() {
            val workers = DelegatingWorkerFactory()
            workers.addFactory(OutboxWorkerFactory { account()?.deliver() ?: true })
            workers.addFactory(WakeWorkerFactory { account()?.catchUp() ?: true })
            return Configuration.Builder().setWorkerFactory(workers).build()
        }

    /** FCM, when the build has the Firebase project's values and the phone has Play services. */
    @FirebaseOnly protected open fun push(): Push? = firebase

    @AndroidKeystoreOnly protected open fun sessionKey(): SecretKey = androidKeystoreKey()
}
