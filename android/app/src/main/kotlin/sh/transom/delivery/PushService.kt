package sh.transom.delivery

import android.annotation.SuppressLint
import androidx.work.WorkManager
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import sh.transom.TransomApplication

/**
 * What FCM hands the app: a wake-up, which the server asked the wake-up gateway to send when an
 * exchange arrived or ended, and each registration. Firebase runs it only in a build with the
 * Firebase project's values, on a phone with Google Play services, once the person chose Google
 * Play services.
 *
 * Lint asks for `onNewToken`, which FCM deprecated along with registration tokens; `onRegistered`
 * takes its place for an app registered by installation ID.
 */
@SuppressLint("MissingFirebaseInstanceTokenRefresh")
class PushService : FirebaseMessagingService() {
    /**
     * A wake-up carries nothing to read, so the phone catches up from its own server, as work that
     * may outlast this call.
     */
    override fun onMessageReceived(message: RemoteMessage) {
        CatchUpWorker.schedule(WorkManager.getInstance(this))
    }

    /**
     * FCM registered the app as [installationId], which is the phone's token from here on: after
     * each registration, as the app starts, and when the ID changes, as after the app's data is
     * cleared. The server is told it each time.
     */
    override fun onRegistered(installationId: String) {
        (application as TransomApplication).newToken(installationId)
    }
}
