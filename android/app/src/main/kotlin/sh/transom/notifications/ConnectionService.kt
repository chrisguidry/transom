package sh.transom.notifications

import android.app.Notification
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import androidx.core.net.toUri
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import sh.transom.Account
import sh.transom.MainActivity
import sh.transom.R
import sh.transom.TransomApplication
import sh.transom.sync.Wake

/**
 * The background connection: while the app is signed in, it follows the account's live feed and
 * keeps its notifications current, so questions arrive with the app closed. This is the
 * `{"connection": true}` device the phone registers as when FCM cannot wake it: in a build without
 * the Firebase project's values, on a phone without Google Play services, or with a server that
 * refuses the token. It shows a quiet, permanent notification while it runs, as Android requires,
 * and it registers the phone as a device, trying again until the server takes the registration, and
 * ends once the server takes an FCM token instead. The person can turn it off in Settings, and
 * questions then arrive only while the app is open.
 *
 * It runs as a `remoteMessaging` foreground service: Android describes that type as transferring
 * text messages from one device to another, which is what holding a connection to the person's own
 * messaging server does. It has no time limit, and Android 15 lets a receiver of `BOOT_COMPLETED`
 * start it. `dataSync` would not do: Android 15 stops a `dataSync` service after six hours in each
 * day, and refuses to start one from `BOOT_COMPLETED`. Google Play asks an app to declare each type
 * it uses and why, and `remoteMessaging` is the type whose description matches.
 */
class ConnectionService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var running: Job? = null
    private var following: Account? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // The manifest names the service's one type, remoteMessaging, which Android 14 and later
        // require, and asking for the manifest's types works on every release since Android 10.
        ServiceCompat.startForeground(
            this,
            NOTIFICATION,
            notification(),
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MANIFEST,
        )
        val account = app.account()
        if (account == null) {
            stopSelf()
            return START_NOT_STICKY
        }
        // A rename replaces the account while the service runs, and the service then follows
        // the new one in its place.
        if (following !== account) {
            running?.cancel()
            following = account
            running = scope.launch {
                account.register()
                // A phone that FCM wakes holds no connection. The phone's boot starts the service
                // in case the server refuses the token, and it ends here once the server takes it.
                launch {
                    account.wakes.first { it is Wake.Fcm }
                    stopSelf()
                }
                launch { account.notifying() }
                // Following ends when the account ends. A rename ends it too, with the session
                // still stored, and the service then follows the account that replaces it. Once
                // the app has signed out, no session is stored, and the service ends.
                account.follow()
                if (app.sessions.session() == null) stopSelf()
            }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private val app: TransomApplication
        get() = application as TransomApplication

    /** The connection's own notification, on its quiet channel, which opens the inbox. */
    private fun notification(): Notification {
        val session = app.sessions.session()
        val host = session?.server?.toUri()?.host.orEmpty()
        val open =
            PendingIntent.getActivity(
                this,
                0,
                Intent(this, MainActivity::class.java),
                PendingIntent.FLAG_IMMUTABLE,
            )
        return NotificationCompat.Builder(this, Channels.CONNECTION)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(getString(R.string.connection_title, host))
            .setContentText(getString(R.string.connection_text, session?.name.orEmpty()))
            .setContentIntent(open)
            .setOngoing(true)
            .setSilent(true)
            .setShowWhen(false)
            .build()
    }

    companion object {
        /** The id of the connection's notification, which no exchange's notification has. */
        const val NOTIFICATION = 2

        /**
         * Starts the background connection. Android lets an app start a foreground service while it
         * is on screen, or from the receivers of the phone's boot and the app's update, which are
         * where this is called.
         */
        fun start(context: Context) {
            ContextCompat.startForegroundService(
                context,
                Intent(context, ConnectionService::class.java),
            )
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, ConnectionService::class.java))
        }
    }
}
