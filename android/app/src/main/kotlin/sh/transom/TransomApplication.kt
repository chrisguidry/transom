package sh.transom

import android.app.Application
import androidx.core.app.NotificationManagerCompat
import androidx.work.Configuration
import androidx.work.DelegatingWorkerFactory
import androidx.work.WorkManager
import java.io.File
import javax.crypto.SecretKey
import okhttp3.OkHttpClient
import sh.transom.delivery.CatchUpWorkerFactory
import sh.transom.delivery.Delivery
import sh.transom.delivery.Firebase
import sh.transom.delivery.FirebaseOnly
import sh.transom.delivery.GooglePlay
import sh.transom.delivery.Push
import sh.transom.delivery.delivery
import sh.transom.notifications.Channels
import sh.transom.notifications.ConnectionService
import sh.transom.notifications.DIRECTORY
import sh.transom.notifications.PICTURES
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
 * with a key of their own, since Robolectric has no Keystore, and give it Google Play services of
 * their own, since Robolectric has none.
 *
 * It gives WorkManager its configuration, in place of the default initializer the manifest removes,
 * so the outbox's worker and the catch-up's are made with the account they work for. WorkManager
 * may start the process for that work alone, with no screen open, so the account is built from the
 * stored session whenever it is first asked for.
 */
open class TransomApplication : Application(), Configuration.Provider {
    val sessions: SessionStore by lazy { SessionStore(noBackupFilesDir, ::sessionKey) }

    val signInRequests: SignInRequests by lazy { SignInRequests(http) }

    val database: SyncDatabase by lazy { SyncDatabase.open(this) }

    val preferences: Preferences by lazy { Preferences(this) }

    private val http: OkHttpClient by lazy { OkHttpClient() }

    private val firebase by lazy { Firebase.googlePlay(this) }

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
        // With Google Play services chosen, Firebase starts with the process, before FCM hands the
        // app a wake-up or a token.
        push()
    }

    /** Whether Google Play services can deliver here, and if not, why. */
    val googlePlay: GooglePlay
        get() = googlePlay()

    /**
     * The delivery the phone uses: the person's choice, or the direct connection while Google Play
     * services is unavailable.
     */
    fun delivery(): Delivery = googlePlay.delivery(preferences.delivery)

    /**
     * Begins what the app does once it is signed in: registering the phone as a device, and, when
     * it delivers by the direct connection, the background connection, which follows the feed and
     * posts the notifications. Each begins once however often this is called.
     */
    fun signedIn() {
        val account = account() ?: return
        account.register()
        if (delivery() == Delivery.Connection) ConnectionService.start(this)
    }

    /** Registers FCM's new [token] for the account signed in. */
    fun newToken(token: String) {
        account()?.registerToken(token)
    }

    /**
     * Delivers by [chosen] from here on, as the person chose in Settings: the phone registers again
     * as the device it now is, and the background connection starts or stops to match. The choice
     * is kept on the phone, and Google Play services is offered only where it is available.
     */
    fun deliverBy(chosen: Delivery) {
        preferences.delivery = chosen
        account()?.deliverBy(push())
        if (delivery() == Delivery.Connection) {
            ConnectionService.start(this)
        } else {
            ConnectionService.stop(this)
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
     * background connection, clears every notification the account posted, and deletes the pictures
     * and images kept for them, so the next person to sign in on this phone sees none of it.
     */
    suspend fun signedOut() {
        val ended = synchronized(this) { account.also { account = null } }
        ConnectionService.stop(this)
        ended?.forget()
        NotificationManagerCompat.from(this).cancelAll()
        File(cacheDir, PICTURES).deleteRecursively()
        File(cacheDir, DIRECTORY).deleteRecursively()
    }

    override val workManagerConfiguration: Configuration
        get() {
            val workers = DelegatingWorkerFactory()
            workers.addFactory(OutboxWorkerFactory { account()?.deliver() ?: true })
            workers.addFactory(CatchUpWorkerFactory { account()?.catchUp() ?: true })
            return Configuration.Builder().setWorkerFactory(workers).build()
        }

    /** FCM, when the person chose Google Play services and the phone has it. */
    private fun push(): Push? =
        (googlePlay as? GooglePlay.Available)?.takeIf { delivery() == Delivery.GooglePlay }?.push

    /** Whether the build has the Firebase project's values and the phone has Play services. */
    @FirebaseOnly protected open fun googlePlay(): GooglePlay = firebase

    @AndroidKeystoreOnly protected open fun sessionKey(): SecretKey = androidKeystoreKey()
}
