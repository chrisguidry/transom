package sh.transom.notifications

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import androidx.core.app.NotificationChannelCompat
import androidx.core.app.NotificationManagerCompat
import sh.transom.R
import sh.transom.exchange.Urgency

/**
 * The app's notification channels: one for each urgency a sender can give, so each person decides
 * what each urgency does on this phone, one for the background connection's own notification, and
 * one for the notification that says Google Play services delivery is failing.
 *
 * A channel's id is permanent. Android restores a deleted channel, with the settings it had, when
 * an app creates one with the same id again, so an id is never reused for a channel with other
 * settings.
 */
object Channels {
    /** The channel of the background connection's permanent, quiet notification. */
    const val CONNECTION = "connection"

    /** The channel of the notification that says why notifications aren't reaching the phone. */
    const val DELIVERY = "delivery"

    /** The urgent channel the app makes while it may not override Do Not Disturb. */
    const val URGENT = "urgent"

    /**
     * The urgent channel that overrides Do Not Disturb. Android sets a channel's override only when
     * the app creates it, and only while the person has given the app access to Do Not Disturb, so
     * this channel is made once they have, and replaces [URGENT].
     */
    const val URGENT_THROUGH_DND = "urgent-dnd"

    /**
     * The channel a message of [urgency] is posted on, named as Web Push names the urgency. An
     * urgent message goes on whichever urgent channel [create] made.
     */
    fun of(urgency: Urgency, context: Context): String =
        when (urgency) {
            Urgency.VERY_LOW -> "very-low"
            Urgency.LOW -> "low"
            Urgency.NORMAL -> "normal"
            Urgency.HIGH -> "high"
            Urgency.URGENT -> urgent(NotificationManagerCompat.from(context))
        }

    private fun urgent(manager: NotificationManagerCompat): String =
        if (manager.getNotificationChannel(URGENT_THROUGH_DND) != null) URGENT_THROUGH_DND
        else URGENT

    /**
     * Creates every channel with its default importance: `urgent` and `high` interrupt with a sound
     * and a heads-up, `normal` makes a sound, `low` makes none, and `very-low` makes none and stays
     * minimized. The delivery problem's channel makes a sound, since questions stop reaching the
     * phone while delivery fails. Android keeps whatever the person has changed since, so creating
     * them again at each start changes only their names and descriptions.
     *
     * Once the person gives the app access to Do Not Disturb, the urgent channel is made again
     * under [URGENT_THROUGH_DND] with the override, and the one before it is deleted. That channel
     * stays if the access is later taken away, so what the person set on it is kept.
     */
    fun create(context: Context) {
        val manager = NotificationManagerCompat.from(context)
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
        manager.createNotificationChannelsCompat(channels)
        val through =
            context
                .getSystemService(NotificationManager::class.java)
                .isNotificationPolicyAccessGranted ||
                manager.getNotificationChannel(URGENT_THROUGH_DND) != null
        // NotificationChannelCompat's builder has no override of Do Not Disturb, so the urgent
        // channel is the platform's own.
        val urgent =
            NotificationChannel(
                    if (through) URGENT_THROUGH_DND else URGENT,
                    context.getString(R.string.channel_urgent),
                    NotificationManager.IMPORTANCE_HIGH,
                )
                .apply {
                    description = context.getString(R.string.channel_urgent_description)
                    setBypassDnd(through)
                }
        manager.createNotificationChannel(urgent)
        if (through && manager.getNotificationChannel(URGENT) != null) {
            manager.deleteNotificationChannel(URGENT)
        }
    }

    private fun channel(context: Context, urgency: Urgency, importance: Int) =
        NotificationChannelCompat.Builder(of(urgency, context), importance)
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
