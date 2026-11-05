package sh.transom.signin

import android.content.pm.PackageManager
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ActivityController
import org.robolectric.annotation.Config
import sh.transom.MainActivity

/**
 * Android 17 refuses the app's connections to the local network until the person grants the local
 * network permission, so a server there that can't be reached has the app ask for it.
 */
@RunWith(RobolectricTestRunner::class)
@Config(shadows = [ShadowPermissionRequests::class])
class SignInLocalNetworkTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val late = LateServer()

    @Test
    fun neverAsksWhenTheServerAnswers() {
        late.start()
        val activity = launch()
        compose.continueTo(late.url)
        compose.customTab()
        assertNull(shadowOf(activity.get()).lastRequestedPermission)
    }

    @Test
    fun asksWhenTheServerCannotBeReached() {
        val activity = launch()
        compose.continueTo(nowhere())
        assertEquals(listOf(LOCAL_NETWORK), compose.permissionsAsked(activity))
    }

    @Test
    fun triesAgainOnceAllowed() {
        val activity = launch()
        compose.continueTo(late.url)
        compose.permissionsAsked(activity)
        late.start()
        shadowOf(app).grantPermissions(LOCAL_NETWORK)
        activity.get().answer(PackageManager.PERMISSION_GRANTED)
        assertEquals("${late.url}/login", compose.customTab().data.toString().substringBefore('?'))
    }

    @Test
    fun saysHowToAllowItWhenRefused() {
        val activity = launch()
        compose.continueTo(nowhere())
        compose.permissionsAsked(activity)
        activity.get().answer(PackageManager.PERMISSION_DENIED)
        compose.awaitText(
            "Signing in to a server on your local network needs permission to reach devices " +
                "there. Allow it in transom's settings, under Permissions, then try again."
        )
    }

    @Test
    fun neverAsksOnceAllowed() {
        shadowOf(app).grantPermissions(LOCAL_NETWORK)
        val activity = launch()
        compose.continueTo(nowhere())
        compose.awaitText("That server can't be reached.", substring = true)
        compose.waitForIdle()
        assertNull(shadowOf(activity.get()).lastRequestedPermission)
    }

    @Test
    fun saysWhyTheServerCannotBeReached() {
        shadowOf(app).grantPermissions(LOCAL_NETWORK)
        launch()
        compose.continueTo(nowhere())
        compose.awaitText("Failed to connect to /127.0.0.1", substring = true)
    }
}

/**
 * The permissions the app asked for, once it has asked. The screen asks once it shows the problem,
 * and waiting for the problem's text is what has Compose draw it: waiting on the request alone
 * leaves the frame undrawn. The system's dialog is the activity the request started, which is taken
 * from the started activities, so the next one is the app's own.
 */
private fun ComposeTestRule.permissionsAsked(
    activity: ActivityController<MainActivity>
): List<String> {
    awaitText("That server can't be reached.", substring = true)
    val shadow = shadowOf(activity.get())
    awaitMain { shadow.lastRequestedPermission != null }
    shadowOf(app).nextStartedActivity
    return shadow.lastRequestedPermission.requestedPermissions.toList()
}
