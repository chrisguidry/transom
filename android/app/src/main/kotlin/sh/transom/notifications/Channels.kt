package sh.transom.notifications

import android.content.Context
import androidx.core.app.NotificationChannelCompat
import androidx.core.app.NotificationManagerCompat
import sh.transom.R
import sh.transom.exchange.Urgency

/**
 * The app's notification channels: one for each urgency a sender can give, so each person decides
 * what each urgency does on this phone, one for the background connection's own notification, and
 * one for the notification that says Google Play services delivery is failing.
 */
object Channels {
    /** The channel of the background connection's permanent, quiet notification. */
    const val CONNECTION = "connection"

    /** The channel of the notification that says why notifications aren't reaching the phone. */
    const val DELIVERY = "delivery"

    /** The channel a message of [urgency] is posted on, named as Web Push names the urgency. */
    fun of(urgency: Urgency): String =
        when (urgency) {
            Urgency.VERY_LOW -> "very-low"
            Urgency.LOW -> "low"
            Urgency.NORMAL -> "normal"
            Urgency.HIGH -> "high"
        }

    /**
     * Creates every channel with its default importance: `high` interrupts with a sound and a
     * heads-up, `normal` makes a sound, `low` makes none, and `very-low` makes none and stays
     * minimized. The delivery problem's channel makes a sound, since questions stop reaching the
     * phone while delivery fails. Android keeps whatever the person has changed since, so creating
     * them again at each start changes only their names and descriptions.
     */
    fun create(context: Context) {
        val channels =
            listOf(
                channel(context, Urgency.HIGH, NotificationManagerCompat.IMPORTANCE_HIGH),
                channel(context, Urgency.NORMAL, NotificationManagerCompat.IMPORTANCE_DEFAULT),
                channel(context, Urgency.LOW, NotificationManagerCompat.IMPORTANCE_LOW),
                channel(context, Urgency.VERY_LOW, NotificationManagerCompat.IMPORTANCE_MIN),
                NotificationChannelCompat.Builder(
                        CONNECTION,
                        NotificationManagerCompat.IMPORTANCE_MIN,
                    )
                    .setName(context.getString(R.string.channel_connection))
                    .setDescription(context.getString(R.string.channel_connection_description))
                    .setShowBadge(false)
                    .build(),
                NotificationChannelCompat.Builder(
                        DELIVERY,
                        NotificationManagerCompat.IMPORTANCE_DEFAULT,
                    )
                    .setName(context.getString(R.string.channel_delivery))
                    .setDescription(context.getString(R.string.channel_delivery_description))
                    .build(),
            )
        NotificationManagerCompat.from(context).createNotificationChannelsCompat(channels)
    }

    private fun channel(context: Context, urgency: Urgency, importance: Int) =
        NotificationChannelCompat.Builder(of(urgency), importance)
            .setName(context.getString(NAMES.getValue(urgency).first))
            .setDescription(context.getString(NAMES.getValue(urgency).second))
            .build()

    private val NAMES =
        mapOf(
            Urgency.HIGH to (R.string.channel_high to R.string.channel_high_description),
            Urgency.NORMAL to (R.string.channel_normal to R.string.channel_normal_description),
            Urgency.LOW to (R.string.channel_low to R.string.channel_low_description),
            Urgency.VERY_LOW to
                (R.string.channel_very_low to R.string.channel_very_low_description),
        )
}
