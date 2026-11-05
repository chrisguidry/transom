package sh.transom.settings

import android.app.NotificationManager
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import sh.transom.LaunchedApp
import sh.transom.delivery.Delivery
import sh.transom.delivery.DeliveryProblem
import sh.transom.delivery.GooglePlay
import sh.transom.delivery.Push
import sh.transom.delivery.REFUSED
import sh.transom.notifications.ConnectionService
import sh.transom.notifications.await
import sh.transom.signin.app
import sh.transom.signin.awaitHeading
import sh.transom.signin.awaitText
import sh.transom.testApp
import sh.transom.testserver.TestServer

/** @alice's choice, in Settings, of how notifications reach her phone. */
@RunWith(RobolectricTestRunner::class)
class DeliverySettingsTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()

    private val googlePlay
        get() = compose.onNodeWithText("Google Play services").performScrollTo()

    private val connection
        get() = compose.onNodeWithText("Direct connection to your server").performScrollTo()

    /** Opens Settings as @alice, with [playServices] as this build on this phone has it. */
    private fun openSettings(playServices: GooglePlay) {
        testApp.playServices = playServices
        launched.signedIn(server, "alice")
        compose.awaitHeading("@alice")
        compose.openSettings()
    }

    private fun openSettings(push: Push) = openSettings(GooglePlay.Available { push })

    @Test
    fun googlePlayServicesIsChosenWhereItIsAvailable() {
        openSettings(Push { "token-1" })
        googlePlay.assertIsSelected()
        connection.assertIsNotSelected()
    }

    @Test
    fun choosingTheDirectConnectionStartsIt() {
        openSettings(Push { "token-1" })
        connection.performClick()
        connection.assertIsSelected()
        assertEquals(
            ConnectionService::class.java.name,
            shadowOf(app).nextStartedService.component?.className,
        )
        assertEquals(Delivery.Connection, app.preferences.delivery)
    }

    @Test
    fun choosingGooglePlayServicesAgainStopsTheConnection() {
        openSettings(Push { "token-1" })
        connection.performClick()
        googlePlay.performClick()
        googlePlay.assertIsSelected()
        assertEquals(
            ConnectionService::class.java.name,
            shadowOf(app).nextStoppedService.component?.className,
        )
        assertEquals(Delivery.GooglePlay, app.preferences.delivery)
    }

    @Test
    fun aBuildWithoutItOffersOnlyTheDirectConnection() {
        openSettings(GooglePlay.Unavailable.NotInBuild)
        googlePlay.assertIsNotEnabled()
        connection.assertIsSelected()
        compose
            .onNodeWithText(
                "This build of transom doesn't include Google Play services delivery.",
                substring = true,
            )
            .assertExists()
    }

    @Test
    fun aPhoneWithoutPlayServicesSaysSo() {
        openSettings(GooglePlay.Unavailable.NotOnPhone)
        googlePlay.assertIsNotEnabled()
        compose
            .onNodeWithText("Google Play services isn't available on this phone.", substring = true)
            .assertExists()
    }

    @Test
    fun aRefusedTokenShowsTheReasonUnderTheChoice() {
        openSettings(Push { REFUSED })
        compose.awaitText("Your server didn't accept this phone for Google Play services delivery.")
    }

    @Test
    fun theProblemsNotificationOpensSettings() {
        testApp.fcm(Push { REFUSED })
        val activity = launched.signedIn(server, "alice")
        compose.awaitHeading("@alice")
        val manager = shadowOf(app.getSystemService(NotificationManager::class.java))
        await { manager.getNotification(DeliveryProblem.NOTIFICATION) != null }
        val notification = manager.getNotification(DeliveryProblem.NOTIFICATION)
        activity.newIntent(shadowOf(notification.contentIntent).savedIntent)
        compose.awaitHeading("Settings")
    }
}
