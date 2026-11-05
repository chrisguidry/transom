package sh.transom.settings

import android.provider.Settings
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ActivityController
import sh.transom.LaunchedApp
import sh.transom.MainActivity
import sh.transom.signin.app
import sh.transom.signin.awaitHeading
import sh.transom.testserver.TestServer

/** @alice's Settings, from the top bar's menu, against a real transom. */
@RunWith(RobolectricTestRunner::class)
class SettingsTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()

    private lateinit var activity: ActivityController<MainActivity>

    @Before
    fun signedIn() {
        activity = launched.signedIn(server, "alice")
        compose.awaitHeading("@alice")
    }

    @Test
    fun theMenuHasSettingsAboveSignOut() {
        compose.onNodeWithContentDescription("Menu").performClick()
        val settings = compose.onNodeWithText("Settings").getBoundsInRoot().top
        val signOut = compose.onNodeWithText("Sign out").getBoundsInRoot().top
        assertTrue(settings < signOut)
    }

    @Test
    fun itShowsTheServer() {
        compose.openSettings()
        compose.onNodeWithText(server.url).assertExists()
    }

    @Test
    fun theNotificationSettingsAreAndroidsOwn() {
        compose.openSettings()
        compose.press("Notification settings")
        assertEquals(
            Settings.ACTION_APP_NOTIFICATION_SETTINGS,
            shadowOf(app).nextStartedActivity.action,
        )
    }

    @Test
    fun itSaysWhenNotificationsAreOff() {
        compose.openSettings()
        compose.onNodeWithText("Notifications are off for this phone.").assertExists()
    }

    @Test
    fun answeringFromTheLockScreenNeedsUnlockingUntilItIsLifted() {
        compose.openSettings()
        val setting = compose.onNodeWithText("Answer from the lock screen").performScrollTo()
        setting.assertIsOff()
        setting.performClick()
        setting.assertIsOn()
        assertTrue(app.preferences.plainActionsWhileLocked)
    }

    @Test
    fun signingOutFromSettingsReturnsToTheSignInScreen() {
        compose.openSettings()
        compose.press("Sign out")
        compose.awaitHeading("Sign in")
    }

    @Test
    fun theLicensesNameEachLibrary() {
        compose.openFromSettings("Open-source licenses", "Licenses")
        compose.onNodeWithText("commonmark-java").performScrollTo()
        compose.onNodeWithText("BSD 2-Clause License").assertIsDisplayed()
    }

    @Test
    fun backFromTheLicensesReturnsToSettings() {
        compose.openFromSettings("Open-source licenses", "Licenses")
        activity.get().onBackPressedDispatcher.onBackPressed()
        compose.awaitHeading("Settings")
    }
}
