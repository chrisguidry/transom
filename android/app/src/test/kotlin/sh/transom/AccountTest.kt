package sh.transom

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.signin.Session
import sh.transom.signin.app
import sh.transom.signin.awaitMain
import sh.transom.signin.awaitText
import sh.transom.signin.callback
import sh.transom.signin.challengeSent
import sh.transom.signin.continueTo
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.field
import sh.transom.testserver.send

/** What the app does as its session, from signing in to signing out, against a real server. */
@RunWith(RobolectricTestRunner::class)
class AccountTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()

    /** Alice asks Bob whether to deploy, and returns the question's id. */
    private fun asked(): String {
        val alice = server.tokenFor("alice")
        val bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
        return server.send(
            alice,
            "bob",
            """{"summary": "Deploy?", "choices": [{"label": "Deploy"}]}""",
        )
    }

    private fun signOut() {
        compose.onNodeWithContentDescription("Menu").performClick()
        compose.onNodeWithText("Sign out").performClick()
    }

    @Test
    fun signingInRegistersThePhoneAsADeviceThatHoldsTheFeedOpen() {
        val activity = launched.launch()
        compose.continueTo(server.url)
        activity.newIntent(callback(server.signInApp("alice", compose.challengeSent())))
        val alice = server.tokenFor("alice")
        compose.awaitMain {
            server.call("GET", "/@alice/devices", alice).contains(""""wake":{"connection":true}""")
        }
    }

    @Test
    fun signingOutClearsTheCopyAndTheOutbox() {
        asked()
        launched.signedIn(server, "bob")
        compose.awaitText("Deploy?")
        compose.onNodeWithText("Deploy?").performClick()
        compose.awaitText("Add a note, if you like")
        compose.onNode(hasText("Deploy") and hasClickAction()).performClick()
        compose.awaitText("@bob replied · sending")
        signOut()
        compose.awaitMain {
            runBlocking {
                val database = app.database
                listOf(database.copies().rows().first(), database.queue().replies()).all {
                    it.isEmpty()
                }
            }
        }
    }

    @Test
    fun theOutboxIsSentWithNoScreenOpen() {
        val id = asked()
        val bob = server.tokenFor("bob")
        app.sessions.keep(Session(server.url, bob, "bob"))
        runBlocking { app.account()!!.outbox.reply(id, choice = "Deploy") }
        compose.awaitOnline {
            runCatching { field(server.call("GET", "/exchanges/$id/reply", bob), "by") }
                .getOrNull() == "@bob"
        }
        assertEquals("Deploy", field(server.call("GET", "/exchanges/$id/reply", bob), "choice"))
    }
}
