package sh.transom.settings

import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performImeAction
import androidx.compose.ui.test.performTextInput
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.LaunchedApp
import sh.transom.signin.Session
import sh.transom.signin.app
import sh.transom.signin.awaitHeading
import sh.transom.signin.awaitMain
import sh.transom.signin.awaitText
import sh.transom.testserver.NOWHERE
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.field
import sh.transom.testserver.send

/** @alice renames herself from Settings, against a real transom. */
@RunWith(RobolectricTestRunner::class)
class RenameTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()

    private lateinit var alice: String

    @Before
    fun people() {
        alice = server.tokenFor("alice")
    }

    private fun rename(to: String) {
        compose.onNodeWithContentDescription("Rename").performTextInput(to)
        compose.press("Rename")
    }

    private fun settings(session: Session = Session(server.url, alice, "alice")) {
        launched.signedIn(session)
        compose.awaitHeading("@alice")
        compose.openSettings()
    }

    @Test
    fun theServerTakesTheNewName() {
        settings()
        rename("alicia")
        compose.awaitText("@alicia")
        assertEquals("alicia", field(server.call("GET", "/@alicia", alice), "name"))
    }

    @Test
    fun theSessionKeepsTheNewName() {
        settings()
        rename("alicia")
        compose.awaitText("@alicia")
        assertEquals("alicia", app.sessions.session()?.name)
    }

    @Test
    fun theKeyboardsGoKeyRenames() {
        settings()
        val field = compose.onNodeWithContentDescription("Rename")
        field.performTextInput("@alicia")
        field.performImeAction()
        compose.awaitMain { app.sessions.session()?.name == "alicia" }
    }

    @Test
    fun theInboxFollowsTheNewName() {
        val bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
        server.send(alice, "bob", """{"summary": "Lunch?"}""")
        settings()
        rename("alicia")
        compose.awaitText("@alicia")
        compose.onNodeWithText("transom").performClick()
        compose.awaitHeading("@alicia")
        compose.awaitText("Lunch?")
        compose.onNodeWithText("Lunch?").performClick()
        compose.awaitText("@alicia to @bob", substring = true)
    }

    @Test
    fun aTakenNameIsRefusedAsTheServerSaysIt() {
        server.tokenFor("bob")
        settings()
        rename("bob")
        compose.awaitText("The name bob is taken.")
    }

    @Test
    fun anInvalidNameIsRefusedAsTheServerSaysIt() {
        settings()
        rename("Alicia")
        compose.awaitText("\"Alicia\" is not a valid name", substring = true)
    }

    @Test
    fun anUnreachableServerIsSaidPlainly() {
        settings(Session(NOWHERE, "transom_token", "alice"))
        rename("alicia")
        compose.awaitText("The server can't be reached. Check your connection and try again.")
    }
}
