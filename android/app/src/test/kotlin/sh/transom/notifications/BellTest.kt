package sh.transom.notifications

import android.Manifest
import android.provider.Settings
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ActivityController
import sh.transom.LaunchedApp
import sh.transom.MainActivity
import sh.transom.signin.app
import sh.transom.signin.awaitMain
import sh.transom.testserver.TestServer

/** The top bar's bell shows whether this phone's notifications are on, and turns them on. */
@RunWith(RobolectricTestRunner::class)
class BellTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()

    private val notifications = Manifest.permission.POST_NOTIFICATIONS

    private fun signedIn(): ActivityController<MainActivity> = launched.signedIn(server, "bob")

    private fun awaitBell(label: String) = compose.awaitMain {
        compose.onAllNodes(hasContentDescription(label)).fetchSemanticsNodes().isNotEmpty()
    }

    /** The permission Android was last asked for, and the code its answer comes back with. */
    private fun ActivityController<MainActivity>.asked() = shadowOf(get()).lastRequestedPermission

    /**
     * Answers the permission's question, as the person does in Android's dialog, which pauses the
     * app while it is open and resumes it once it closes.
     */
    private fun ActivityController<MainActivity>.answer(granted: Boolean) {
        pause()
        if (granted) shadowOf(app).grantPermissions(notifications)
        resume()
    }

    @Test
    fun signingInAsksForNotifications() {
        val activity = signedIn()
        awaitBell("Notifications off")
        assertEquals(listOf(notifications), activity.asked().requestedPermissions.toList())
    }

    @Test
    fun theBellIsOnOnceThePersonAllowsThem() {
        val activity = signedIn()
        awaitBell("Notifications off")
        activity.answer(granted = true)
        awaitBell("Notifications on")
    }

    @Test
    fun theBellAsksAgainWhileAndroidStillWould() {
        val activity = signedIn()
        awaitBell("Notifications off")
        activity.answer(granted = false)
        shadowOf(app.packageManager).setShouldShowRequestPermissionRationale(notifications, true)
        val first = activity.asked()
        compose.onNodeWithContentDescription("Notifications off").performClick()
        assertNotSame(first, activity.asked())
    }

    @Test
    fun theBellOpensTheSettingsOnceAndroidWouldNotAsk() {
        val activity = signedIn()
        awaitBell("Notifications off")
        activity.answer(granted = false)
        compose.onNodeWithContentDescription("Notifications off").performClick()
        assertEquals(
            Settings.ACTION_APP_NOTIFICATION_SETTINGS,
            shadowOf(app).nextStartedActivity.action,
        )
    }

    @Test
    fun theBellOpensTheSettingsToTurnThemOff() {
        shadowOf(app).grantPermissions(notifications)
        signedIn()
        awaitBell("Notifications on")
        compose.onNodeWithContentDescription("Notifications on").performClick()
        assertEquals(
            Settings.ACTION_APP_NOTIFICATION_SETTINGS,
            shadowOf(app).nextStartedActivity.action,
        )
    }
}
