package sh.transom.notifications

import android.content.Context
import androidx.core.app.NotificationChannelCompat
import androidx.core.app.NotificationManagerCompat
import sh.transom.R
import sh.transom.exchange.Urgency

/**
 * The app's notification channels: one for each urgency a sender can give, so each person decides
 * what each urgency does on this phone, and one for the background connection's own notification.
 */
object Channels {
    /** The channel of the background connection's permanent, quiet notification. */
    const val CONNECTION = "connection"

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
     * minimized. Android keeps whatever the person has changed since, so creating them again at
     * each start changes only their names and descriptions.
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
