package sh.transom.signin

import android.content.Intent
import android.net.Uri
import android.os.Looper
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ApplicationProvider
import org.robolectric.Robolectric
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ActivityController
import sh.transom.MainActivity
import sh.transom.TransomApplication

/** The app the test runs in, whose session store is the one the activity reads. */
val app: TransomApplication
    get() = ApplicationProvider.getApplicationContext()

/**
 * The app's activity, started for [intent] as Android starts it: by default from the launcher.
 * Robolectric's controller can then hand it a new intent, as the App Link does.
 */
fun launch(
    intent: Intent = Intent(app, MainActivity::class.java)
): ActivityController<MainActivity> =
    Robolectric.buildActivity(MainActivity::class.java, intent).setup()

/** The App Link the Custom Tab follows when the server finishes a sign-in with [code]. */
fun callback(code: String): Intent =
    Intent(Intent.ACTION_VIEW, Uri.parse("$APP_CALLBACK?code=${Uri.encode(code)}"))

/** Types [server] on the sign-in screen and presses Continue. */
fun ComposeTestRule.continueTo(server: String) {
    onNode(hasSetTextAction()).performTextInput(server)
    onNodeWithText("Continue").performClick()
}

/** The Custom Tab the app opened, once it has. */
fun ComposeTestRule.customTab(): Intent {
    awaitMain { shadowOf(app).peekNextStartedActivity() != null }
    return shadowOf(app).nextStartedActivity
}

/** The PKCE challenge the app sent with the sign-in page the Custom Tab opened. */
fun ComposeTestRule.challengeSent(): String = customTab().data!!.getQueryParameter("app")!!

/**
 * Waits for [text], or text with [text] in it when [substring], to show, as it does once the server
 * answers.
 */
fun ComposeTestRule.awaitText(text: String, substring: Boolean = false) = awaitMain {
    onAllNodes(hasText(text, substring)).fetchSemanticsNodes().isNotEmpty()
}

/**
 * Waits for [condition], running what the main thread has queued before each look. A request's
 * answer comes back to the main thread as a queued task, which Compose's own waiting leaves queued
 * when nothing on screen is waiting to be drawn.
 */
fun ComposeTestRule.awaitMain(condition: () -> Boolean) =
    waitUntil(10_000) {
        shadowOf(Looper.getMainLooper()).idle()
        condition()
    }
