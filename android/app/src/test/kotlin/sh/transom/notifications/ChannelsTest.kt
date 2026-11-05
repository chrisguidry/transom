package sh.transom.notifications

import android.app.NotificationChannel
import android.app.NotificationManager
import androidx.core.app.NotificationManagerCompat
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import sh.transom.exchange.Urgency
import sh.transom.signin.app

/**
 * The app makes a channel for each urgency, one for the background connection, and one for a
 * delivery problem, at start.
 */
@RunWith(RobolectricTestRunner::class)
class ChannelsTest {
    private val manager
        get() = app.getSystemService(NotificationManager::class.java)

    private fun channel(id: String): NotificationChannel? = manager.getNotificationChannel(id)

    private fun importance(channel: String) =
        NotificationManagerCompat.from(app).getNotificationChannelCompat(channel)?.importance

    @Test
    fun eachUrgencyHasTheImportanceItIsNamedFor() =
        assertEquals(
            listOf(
                NotificationManagerCompat.IMPORTANCE_HIGH,
                NotificationManagerCompat.IMPORTANCE_DEFAULT,
                NotificationManagerCompat.IMPORTANCE_LOW,
                NotificationManagerCompat.IMPORTANCE_MIN,
            ),
            listOf("high", "normal", "low", "very-low").map(::importance),
        )

    @Test
    fun urgentInterruptsButCannotYetOverrideDoNotDisturb() {
        val urgent = channel(Channels.URGENT)
        assertEquals(
            listOf(NotificationManagerCompat.IMPORTANCE_HIGH, false),
            listOf(urgent?.importance, urgent?.canBypassDnd()),
        )
    }

    @Test
    fun accessToDoNotDisturbMovesUrgentToANewChannelThatOverridesIt() {
        shadowOf(manager).setNotificationPolicyAccessGranted(true)
        Channels.create(app)
        assertEquals(
            listOf(null, true, Channels.URGENT_THROUGH_DND),
            listOf(
                channel(Channels.URGENT),
                channel(Channels.URGENT_THROUGH_DND)?.canBypassDnd(),
                Channels.of(Urgency.URGENT, app),
            ),
        )
    }

    @Test
    fun theChannelThatOverridesDoNotDisturbStaysIfAccessIsWithdrawn() {
        shadowOf(manager).setNotificationPolicyAccessGranted(true)
        Channels.create(app)
        shadowOf(manager).setNotificationPolicyAccessGranted(false)
        Channels.create(app)
        assertEquals(
            listOf(null, Channels.URGENT_THROUGH_DND),
            listOf(channel(Channels.URGENT), Channels.of(Urgency.URGENT, app)),
        )
    }

    @Test
    fun theConnectionIsQuiet() =
        assertEquals(NotificationManagerCompat.IMPORTANCE_MIN, importance(Channels.CONNECTION))

    @Test
    fun aDeliveryProblemMakesASound() =
        assertEquals(NotificationManagerCompat.IMPORTANCE_DEFAULT, importance(Channels.DELIVERY))
}
