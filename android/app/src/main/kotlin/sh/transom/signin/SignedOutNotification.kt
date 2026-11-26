package sh.transom.signin

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
 * The one notification that says the server ended the session, so notifications stopped until the
 * person signs in again, and how many queued replies were dropped with it. It opens the app on the
 * sign-in screen, and goes once the person signs in again.
 */
class SignedOutNotification(private val context: Context) {
    private val manager = NotificationManagerCompat.from(context)

    /** Posts the notification, saying that [dropped] replies were never sent. */
    // Android drops a notification posted without the permission to post one, as it does the
    // exchanges', and the bell in the top bar asks for it.
    @SuppressLint("MissingPermission")
    fun show(dropped: Int) {
        val again = context.getString(R.string.signed_out_text)
        val text =
            if (dropped == 0) again
            else {
                "$again " +
                    context.resources.getQuantityString(
                        R.plurals.signed_out_dropped,
                        dropped,
                        dropped,
                    )
            }
        val notification =
            NotificationCompat.Builder(context, Channels.DELIVERY)
                .setSmallIcon(R.drawable.ic_notification)
                .setContentTitle(context.getString(R.string.signed_out_title))
                .setContentText(text)
                .setStyle(NotificationCompat.BigTextStyle().bigText(text))
                .setContentIntent(
                    PendingIntent.getActivity(
                        context,
                        NOTIFICATION,
                        Intent(context, MainActivity::class.java),
                        PendingIntent.FLAG_IMMUTABLE,
                    ),
                )
                .setAutoCancel(true)
                .build()
        manager.notify(NOTIFICATION, notification)
    }

    fun cancel() {
        manager.cancel(NOTIFICATION)
    }

    companion object {
        /** The id of the notification, which no other notification has. */
        const val NOTIFICATION = 5
    }
}
