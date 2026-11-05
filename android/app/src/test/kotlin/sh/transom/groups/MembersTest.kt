package sh.transom.groups

import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performFirstLinkClick
import androidx.compose.ui.test.performImeAction
import androidx.compose.ui.test.performTextInput
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.LaunchedApp
import sh.transom.settings.press
import sh.transom.signin.awaitHeading
import sh.transom.signin.awaitMain
import sh.transom.signin.awaitText
import sh.transom.testserver.TestServer
import sh.transom.testserver.call
import sh.transom.testserver.group
import sh.transom.testserver.send

/** @bob opens the members of @team, which @alice made, from a question sent to the group. */
@RunWith(RobolectricTestRunner::class)
class MembersTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()

    private lateinit var alice: String
    private lateinit var bob: String

    @Before
    fun openTheMembers() {
        alice = server.tokenFor("alice")
        bob = server.tokenFor("bob")
        server.group("team", alice, "bob" to bob)
        server.send(alice, "team", """{"summary": "Deploy?", "choices": ["Deploy"]}""")
        launched.signedIn(server, "bob")
        compose.awaitText("Deploy?")
        compose.onNodeWithText("Deploy?").performClick()
        compose.awaitHeading("Deploy?")
        compose.onNodeWithText("@team", substring = true).performFirstLinkClick()
        compose.awaitHeading("@team")
        compose.awaitText("@alice")
    }

    private fun members(): String = server.call("GET", "/@team/members", alice)

    private fun invite(name: String) {
        compose.onNodeWithContentDescription("Invite").performTextInput(name)
        compose.press("Invite")
    }

    @Test
    fun othersCanBeRemovedAndYouCanLeave() {
        compose.onNodeWithText("Remove").assertExists()
        compose.onNodeWithText("Leave").assertExists()
    }

    @Test
    fun anInvitationAsksTheInviteeFirst() {
        val carol = server.tokenFor("carol")
        invite("carol")
        compose.awaitText("Invited @carol. They join when they accept.")
        val asked = server.call("GET", "/@carol/exchanges?state=open&question=true", carol)
        assertEquals(true, asked.contains(""""from":"@transom""""))
        assertFalse("@carol" in members())
    }

    @Test
    fun theKeyboardsGoKeyInvites() {
        server.tokenFor("carol")
        val field = compose.onNodeWithContentDescription("Invite")
        field.performTextInput("@carol")
        field.performImeAction()
        compose.awaitText("Invited @carol. They join when they accept.")
    }

    @Test
    fun invitingAMemberSaysSo() {
        invite("alice")
        compose.awaitText("@alice is a member already.")
    }

    @Test
    fun invitingNoOneSaysWhy() {
        invite("nobody")
        compose.awaitText("No one is named nobody.")
    }

    @Test
    fun removingAMemberTakesThemOut() {
        compose.press("Remove")
        compose.awaitMain { compose.onAllNodes(hasText("@alice")).fetchSemanticsNodes().isEmpty() }
        assertEquals("""{"members":["@bob"]}""", server.call("GET", "/@team/members", bob))
    }

    @Test
    fun leavingReturnsToTheInbox() {
        compose.press("Leave")
        compose.awaitHeading("@bob")
        assertFalse("@bob" in members())
    }
}
