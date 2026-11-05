package sh.transom.delivery

import android.app.Notification
import android.app.NotificationManager
import android.content.Context
import androidx.core.app.NotificationCompat
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import sh.transom.notifications.Channels
import sh.transom.notifications.await
import sh.transom.signin.app
import sh.transom.testserver.TestServer

/**
 * While Google Play services delivery fails, one notification says why, and it goes once delivery
 * works or the phone holds its connection instead.
 */
@RunWith(RobolectricTestRunner::class)
class DeliveryProblemTest {
    @get:Rule val server = TestServer()
    @get:Rule val bob = BobsAccount(server)

    private val manager: NotificationManager
        get() = app.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

    /** The problem's notification, or `null` while none is posted. */
    private fun problem(): Notification? =
        shadowOf(manager).getNotification(DeliveryProblem.NOTIFICATION)

    private fun awaitProblem(): Notification {
        await { problem() != null }
        return problem()!!
    }

    private val Notification.text: String?
        get() = extras.getCharSequence(NotificationCompat.EXTRA_TEXT)?.toString()

    @Test
    fun aRefusedTokenSaysTheServerDidNotAcceptThePhone() {
        bob.registered(Push { REFUSED })
        val shown = awaitProblem()
        assertEquals(
            "Notifications aren't working",
            shown.extras.getString(NotificationCompat.EXTRA_TITLE),
        )
        assertEquals(
            "Your server didn't accept this phone for Google Play services delivery.",
            shown.text,
        )
        assertEquals(Channels.DELIVERY, shown.channelId)
    }

    @Test
    fun noTokenSaysGooglePlayServicesCouldNotRegisterThePhone() {
        bob.registered(Push { null })
        assertEquals("Google Play services couldn't register this phone.", awaitProblem().text)
    }

    @Test
    fun tappingItOpensSettings() {
        bob.registered(Push { REFUSED })
        val tap = shadowOf(awaitProblem().contentIntent).savedIntent
        assertEquals(true, tap.getBooleanExtra(OPEN_SETTINGS, false))
    }

    @Test
    fun itGoesOnceDeliveryWorks() {
        val made = bob.registered(Push { REFUSED })
        awaitProblem()
        runBlocking { made.registerToken("token-1")?.join() }
        await { problem() == null }
    }

    @Test
    fun itGoesOnSwitchingToTheConnection() {
        val made = bob.registered(Push { REFUSED })
        awaitProblem()
        runBlocking { made.deliverBy(null).join() }
        await { problem() == null }
    }

    @Test
    fun itStaysWhileTheRegistrationSettlesAgain() {
        val made = bob.registered(Push { REFUSED })
        awaitProblem()
        made.deliverBy(Push { awaitCancellation() })
        await { made.delivery.value == DeliveryState.Registering }
        assertNotNull(problem())
    }
}
