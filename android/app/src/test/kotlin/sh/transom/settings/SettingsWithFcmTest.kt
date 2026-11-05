package sh.transom.settings

import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.LaunchedApp
import sh.transom.signin.awaitHeading
import sh.transom.signin.awaitText
import sh.transom.testApp
import sh.transom.testserver.TestServer
import sh.transom.wake.Push

/** @alice's Settings on a phone that FCM wakes, which holds no connection to turn off. */
@RunWith(RobolectricTestRunner::class)
class SettingsWithFcmTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()

    @Test
    fun itSaysThePhoneWakesThroughGooglesPushServiceInPlaceOfTheSwitch() {
        testApp.fcm = Push { "token-1" }
        launched.signedIn(server, "alice")
        compose.awaitHeading("@alice")
        compose.openSettings()
        compose.awaitText("This phone wakes through Google's push service", substring = true)
        assertTrue(
            compose.onAllNodes(hasText("Background connection")).fetchSemanticsNodes().isEmpty()
        )
    }
}
