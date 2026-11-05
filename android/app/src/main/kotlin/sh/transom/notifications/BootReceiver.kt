package sh.transom.notifications

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import sh.transom.TransomApplication

/**
 * Starts the background connection when the phone boots and after the app is updated, which ends
 * the app's process and its service with it, while the app is signed in and the person has left the
 * connection on. It starts it with FCM too, since only registering says whether the server takes
 * the token, and Android lets an app start a foreground service from these broadcasts but not later
 * from the background. The connection ends once the server takes the token.
 */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action !in STARTS) return
        val app = context.applicationContext as TransomApplication
        if (app.sessions.session() != null && app.preferences.backgroundConnection) {
            ConnectionService.start(context)
        }
    }
}

private val STARTS = setOf(Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_MY_PACKAGE_REPLACED)
