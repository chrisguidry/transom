package sh.transom.delivery

import android.content.Intent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import sh.transom.notifications.BootReceiver
import sh.transom.notifications.Connected
import sh.transom.notifications.ConnectionService
import sh.transom.notifications.await
import sh.transom.settings.Preferences
import sh.transom.signin.Session
import sh.transom.signin.app
import sh.transom.testApp
import sh.transom.testserver.TestServer
import sh.transom.testserver.call

/**
 * The person's choice decides how notifications reach the phone: Google Play services by default
 * where it is available, the direct connection otherwise, and never one standing in for the other.
 */
@RunWith(RobolectricTestRunner::class)
class ChoosingDeliveryTest {
    @get:Rule val server = TestServer()
    @get:Rule val connected = Connected()

    private val bob by lazy { server.tokenFor("bob") }

    private fun signedIn() {
        app.sessions.keep(Session(server.url, bob, "bob"))
        app.signedIn()
    }

    private fun devices(): String = server.call("GET", "/@bob/devices", bob)

    private val connectionStarted: Boolean
        get() =
            shadowOf(app).peekNextStartedService()?.component?.className ==
                ConnectionService::class.java.name

    @Test
    fun googlePlayIsTheDefaultWhereItIsAvailable() {
        testApp.fcm(Push { "token-1" })
        assertEquals(Delivery.GooglePlay, app.delivery())
    }

    @Test
    fun withoutGooglePlayThePhoneUsesTheConnectionWhateverTheChoice() {
        testApp.playServices = GooglePlay.Unavailable.NotOnPhone
        signedIn()
        assertEquals(Delivery.Connection, app.delivery())
        assertTrue(connectionStarted)
    }

    @Test
    fun aRefusedTokenLeavesTheConnectionStopped() {
        testApp.fcm(Push { REFUSED })
        signedIn()
        await { app.account()?.delivery?.value == DeliveryState.Failing.Refused }
        assertNull(shadowOf(app).nextStartedService)
    }

    @Test
    fun theBootLeavesTheConnectionStoppedWithGooglePlay() {
        testApp.fcm(Push { "token-1" })
        app.sessions.keep(Session(server.url, bob, "bob"))
        BootReceiver().onReceive(app, Intent(Intent.ACTION_BOOT_COMPLETED))
        assertNull(shadowOf(app).nextStartedService)
    }

    @Test
    fun choosingTheConnectionStartsItAndRegistersThePhoneForIt() {
        testApp.fcm(Push { "token-1" })
        signedIn()
        await { app.account()?.delivery?.value is DeliveryState.Delivering }
        app.deliverBy(Delivery.Connection)
        assertTrue(connectionStarted)
        await { devices().contains(""""wake":{"connection":true}""") }
    }

    @Test
    fun choosingGooglePlayStopsTheConnectionAndRegistersTheToken() {
        testApp.fcm(Push { "token-1" })
        app.preferences.delivery = Delivery.Connection
        signedIn()
        app.deliverBy(Delivery.GooglePlay)
        assertEquals(
            ConnectionService::class.java.name,
            shadowOf(app).nextStoppedService.component?.className,
        )
        await { devices().contains(""""wake":{"fcm":"token-1"}""") }
    }

    @Test
    fun theChoiceIsKeptOnThePhone() {
        testApp.fcm(Push { "token-1" })
        app.deliverBy(Delivery.Connection)
        assertEquals(Delivery.Connection, Preferences(app).delivery)
    }

    @Test
    fun aNewTokenWithTheConnectionChosenIsNotRegistered() {
        testApp.fcm(Push { "token-1" })
        app.preferences.delivery = Delivery.Connection
        signedIn()
        await { devices().contains(""""wake":{"connection":true}""") }
        app.newToken("token-2")
        assertFalse(devices().contains("token-2"))
    }
}
