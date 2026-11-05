package sh.transom.settings

import android.app.NotificationManager
import android.provider.Settings
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollTo
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import sh.transom.LaunchedApp
import sh.transom.notifications.ShadowFullScreen
import sh.transom.signin.app
import sh.transom.signin.awaitHeading
import sh.transom.testserver.TestServer

/** Settings asks for what an urgent message needs, and says once Android grants it. */
@RunWith(RobolectricTestRunner::class)
@Config(shadows = [ShadowFullScreen::class])
class UrgentSettingsTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()

    private fun settings(fullScreen: Boolean = true, doNotDisturb: Boolean = false) {
        ShadowFullScreen.granted = fullScreen
        val manager = app.getSystemService(NotificationManager::class.java)
        shadowOf(manager).setNotificationPolicyAccessGranted(doNotDisturb)
        launched.signedIn(server, "alice")
        compose.awaitHeading("@alice")
        compose.openSettings()
    }

    /** Presses the one Allow, beside [text], which says what it allows. */
    private fun allow(text: String) {
        compose.onNodeWithText(text).performScrollTo()
        compose.press("Allow")
    }

    @Test
    fun asksForDoNotDisturbAccess() {
        settings(fullScreen = true)
        allow(DND_OFF)
        assertEquals(
            Settings.ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS,
            shadowOf(app).nextStartedActivity.action,
        )
    }

    @Test
    fun saysWhenUrgentMessagesOverrideDoNotDisturb() {
        settings(doNotDisturb = true)
        compose
            .onNodeWithText("Allowed. Urgent alerts ring through Do Not Disturb.")
            .performScrollTo()
    }

    @Test
    fun asksToShowUrgentMessagesFullScreen() {
        settings(fullScreen = false, doNotDisturb = true)
        allow(FULL_SCREEN_OFF)
        val opened = shadowOf(app).nextStartedActivity
        assertEquals(
            listOf(Settings.ACTION_MANAGE_APP_USE_FULL_SCREEN_INTENT, "package:sh.transom"),
            listOf(opened.action, opened.dataString),
        )
    }

    @Test
    fun saysWhenUrgentMessagesShowFullScreen() {
        settings(fullScreen = true)
        compose
            .onNodeWithText("Allowed. Urgent alerts fill the screen when the phone is locked.")
            .performScrollTo()
    }

    private companion object {
        const val DND_OFF = "Urgent alerts can wake you while Do Not Disturb is on."
        const val FULL_SCREEN_OFF =
            "Urgent alerts fill the screen, even when the phone is locked. Until you allow it, they show as a banner."
    }
}
