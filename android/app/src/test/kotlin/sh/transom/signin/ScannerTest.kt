package sh.transom.signin

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import sh.transom.theme.TransomTheme

/**
 * Scan, beside the server's field, reads the server's URL from the web's QR code. The camera here
 * shows a code of the test's choosing, since Robolectric has no camera.
 */
@RunWith(RobolectricTestRunner::class)
@Config(shadows = [ShadowPermissionRequests::class])
class ScannerTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    private val continued = mutableListOf<String>()

    /** The sign-in screen on a phone with a camera, whose frames show a code of [code], if any. */
    private fun signInShowing(code: String?) {
        shadowOf(app.packageManager).setSystemFeature(PackageManager.FEATURE_CAMERA_ANY, true)
        compose.setContent {
            TransomTheme {
                SignInScreen(
                    problem = null,
                    working = false,
                    onContinue = { continued += it },
                    camera = showing(code),
                )
            }
        }
    }

    private fun allowCamera() = shadowOf(app).grantPermissions(Manifest.permission.CAMERA)

    @Test
    fun scanAsksForTheCamera() {
        signInShowing(null)
        compose.onNodeWithText("Scan").performClick()
        compose.waitForIdle()
        assertEquals(
            listOf(Manifest.permission.CAMERA),
            shadowOf(compose.activity).lastRequestedPermission.requestedPermissions.toList(),
        )
    }

    @Test
    fun saysSoWhenTheCameraIsRefused() {
        signInShowing(null)
        compose.onNodeWithText("Scan").performClick()
        compose.waitForIdle()
        compose.activity.answer(PackageManager.PERMISSION_DENIED)
        compose.awaitText(
            "Scanning a code needs the camera. Allow it in transom's settings, under " +
                "Permissions, or type the server's address."
        )
    }

    @Test
    fun opensTheCameraOnceAllowed() {
        signInShowing(null)
        compose.onNodeWithText("Scan").performClick()
        compose.waitForIdle()
        allowCamera()
        compose.activity.answer(PackageManager.PERMISSION_GRANTED)
        compose.awaitText(POINT)
    }

    @Test
    fun neverAsksOnceAllowed() {
        allowCamera()
        signInShowing(null)
        compose.onNodeWithText("Scan").performClick()
        compose.awaitText(POINT)
        assertNull(shadowOf(compose.activity).lastRequestedPermission)
    }

    @Test
    fun fillsInTheScannedServer() {
        allowCamera()
        signInShowing("http://127.0.0.1:8080/")
        compose.onNodeWithText("Scan").performClick()
        compose.waitForIdle()
        compose.onNode(hasSetTextAction()).assertTextContains("http://127.0.0.1:8080")
    }

    @Test
    fun waitsForContinueAfterScanning() {
        allowCamera()
        signInShowing("http://127.0.0.1:8080")
        compose.onNodeWithText("Scan").performClick()
        compose.waitForIdle()
        assertEquals(emptyList<String>(), continued)
        compose.onNodeWithText("Continue").performClick()
        assertEquals(listOf("http://127.0.0.1:8080"), continued)
    }

    @Test
    fun saysSoWhenTheCodeIsNoServer() {
        allowCamera()
        signInShowing("WIFI:S:home;T:WPA;P:secret;;")
        compose.onNodeWithText("Scan").performClick()
        compose.awaitText(
            "That code isn't a server's address. Scan the one under Pair a phone, on your " +
                "transom's Devices page."
        )
    }

    @Test
    fun cancelGoesBackToSignIn() {
        allowCamera()
        signInShowing(null)
        compose.onNodeWithText("Scan").performClick()
        compose.onNodeWithText("Cancel").performClick()
        compose.onAllNodesWithText(POINT).assertCountEquals(0)
        compose.onNodeWithText("Continue").assertExists()
    }

    @Test
    fun backGoesBackToSignIn() {
        allowCamera()
        signInShowing(null)
        compose.onNodeWithText("Scan").performClick()
        compose.waitForIdle()
        compose.activity.onBackPressedDispatcher.onBackPressed()
        compose.onAllNodesWithText(POINT).assertCountEquals(0)
        compose.onNodeWithText("Continue").assertExists()
    }

    @Test
    fun offersNoScanWithoutACamera() {
        compose.setContent {
            TransomTheme { SignInScreen(problem = null, working = false, onContinue = {}) }
        }
        compose.onAllNodesWithText("Scan").assertCountEquals(0)
    }
}

private const val POINT =
    "Point the camera at the code under Pair a phone, on your transom's Devices page."

/** A camera whose frames show a QR code of [code], or no code at all when it is null. */
private fun showing(code: String?) = Camera { onText, modifier ->
    Box(modifier)
    LaunchedEffect(Unit) { code?.let(onText) }
}
