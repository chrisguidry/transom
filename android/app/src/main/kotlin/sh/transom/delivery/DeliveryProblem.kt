package sh.transom.delivery

import android.annotation.SuppressLint
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import sh.transom.MainActivity
import sh.transom.R
import sh.transom.notifications.Channels

/**
 * The one notification that says Google Play services delivery is failing, with the reason, which
 * opens Settings. It stays while delivery fails, and goes once it works or the phone holds its
 * connection instead.
 */
class DeliveryProblem(private val context: Context) {
    private val manager = NotificationManagerCompat.from(context)

    /**
     * Makes the notification what [state] says. While the registration settles, it stays as it is,
     * so registering again after a restart neither clears it nor makes it alert a second time.
     */
    // Android drops a notification posted without the permission to post one, as it does the
    // exchanges', and the bell in the top bar asks for it.
    @SuppressLint("MissingPermission")
    fun show(state: DeliveryState) {
        when (state) {
            is DeliveryState.Failing -> manager.notify(NOTIFICATION, notification(state))
            DeliveryState.Registering -> {}
            else -> manager.cancel(NOTIFICATION)
        }
    }

    private fun notification(failing: DeliveryState.Failing) =
        NotificationCompat.Builder(context, Channels.DELIVERY)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(context.getString(R.string.delivery_failing))
            .setContentText(context.getString(failing.reason))
            .setContentIntent(
                PendingIntent.getActivity(
                    context,
                    NOTIFICATION,
                    Intent(context, MainActivity::class.java).putExtra(OPEN_SETTINGS, true),
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                )
            )
            // Posted again with the same reason, it changes nothing the person notices.
            .setOnlyAlertOnce(true)
            .build()

    companion object {
        /** The id of the notification, which no other notification has. */
        const val NOTIFICATION = 4
    }
}

/** The extra of an intent that opens Settings, as the delivery problem's notification does. */
const val OPEN_SETTINGS = "sh.transom.extra.SETTINGS"
