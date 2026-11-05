package sh.transom.exchange

import android.content.Intent
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isHeading
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import sh.transom.LaunchedApp
import sh.transom.signin.app
import sh.transom.signin.awaitMain
import sh.transom.signin.awaitText
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.send

/** A message's body and links on its screen, as @bob reads them on a real server. */
@RunWith(RobolectricTestRunner::class)
class BodyTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()

    /** @alice sends @bob [message], and Bob opens it by its [summary]. */
    private fun opened(summary: String, message: String) {
        val alice = server.tokenFor("alice")
        val bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
        server.send(alice, "bob", message)
        launched.signedIn(server, "bob")
        compose.awaitText(summary)
        compose.onNodeWithText(summary).performClick()
        compose.awaitMain {
            compose.onAllNodes(hasText(summary) and isHeading()).fetchSemanticsNodes().isNotEmpty()
        }
    }

    /** The address of the page Android was asked to open, once it has been. */
    private fun opening(): String {
        compose.awaitMain { shadowOf(app).peekNextStartedActivity() != null }
        val intent = shadowOf(app).nextStartedActivity
        assertEquals(Intent.ACTION_VIEW, intent.action)
        return intent.dataString!!
    }

    /** On a screen tall enough to show the whole body without scrolling. */
    @Test
    @Config(qualifiers = "w411dp-h2000dp")
    fun drawsEveryKindOfBlock() {
        opened("Release", message("Release", body = EVERYTHING))
        for (text in DRAWN) compose.onNodeWithText(text).assertIsDisplayed()
    }

    @Test
    fun opensALinkInTheBodyInTheBrowser() {
        opened(
            "Graphs",
            message("Graphs", body = "[the dashboard](https://grafana.example.com/d/1)"),
        )
        compose.onNodeWithText("the dashboard").performClick()
        assertEquals("https://grafana.example.com/d/1", opening())
    }

    @Test
    fun opensALinkToTransomOnTheSessionsServer() {
        opened("See", message("See", body = "[the other one](/exchanges/01JB)"))
        compose.onNodeWithText("the other one").performClick()
        assertEquals("${server.url}/exchanges/01JB", opening())
    }

    @Test
    fun opensTheMessagesLinksInTheBrowser() {
        opened("Built", message("Built", links = listOf("https://ci.example.com/1" to "related")))
        compose.onNodeWithText("related: https://ci.example.com/1").performClick()
        assertEquals("https://ci.example.com/1", opening())
    }

    private companion object {
        val EVERYTHING =
            """
            # Notes

            ## Details

            ### Steps

            1. Build
            2. Test

            - [x] Built
            - [ ] Shipped
            - Plain

            > Quoted

            ```
            make release
            ```

            | Host | Disk |
            | --- | --- |
            | nuc3 | 91% |

            ---

            The end.
            """
                .trimIndent()

        val DRAWN =
            listOf(
                "Notes",
                "DETAILS",
                "Steps",
                "1.",
                "Build",
                "☑",
                "☐",
                "Quoted",
                "make release",
                "Host",
                "91%",
                "The end.",
            )
    }
}
