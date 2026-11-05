package sh.transom.exchange

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isHeading
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.LaunchedApp
import sh.transom.signin.Session
import sh.transom.signin.app
import sh.transom.signin.awaitMain
import sh.transom.signin.awaitText
import sh.transom.sync.Client
import sh.transom.testserver.NOWHERE
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.field
import sh.transom.testserver.send

/** @alice withdraws her own open question to @bob from its screen, on a real server. */
@RunWith(RobolectricTestRunner::class)
class WithdrawTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()

    private lateinit var alice: String
    private lateinit var bob: String
    private lateinit var id: String

    @Before
    fun aliceAsks() {
        alice = server.tokenFor("alice")
        bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
        id = server.send(alice, "bob", message("Deploy?", choices = listOf("Deploy" to null)))
    }

    /** The question, opened from the inbox of the app signed in with [session]. */
    private fun open(session: Session) {
        launched.signedIn(session)
        compose.awaitText("Deploy?")
        compose.onNodeWithText("Deploy?").performClick()
        compose.awaitMain {
            compose
                .onAllNodes(hasText("Deploy?") and isHeading())
                .fetchSemanticsNodes()
                .isNotEmpty()
        }
    }

    /**
     * The question as Alice's phone copied it, with the app signed in to [address] with [token], so
     * the screen shows it whether or not that server answers.
     */
    private fun copied(address: String, token: String) {
        val exchange = runBlocking { Client(server.url, alice).exchange(id) }
        runBlocking { app.database.copies().save(exchange, "alice") }
        open(Session(address, token, "alice"))
    }

    @Test
    fun theSenderCanWithdrawHerOpenQuestion() {
        open(Session(server.url, alice, "alice"))
        compose.onNodeWithText("Withdraw").assertIsDisplayed()
    }

    @Test
    fun theRecipientCannotWithdrawIt() {
        open(Session(server.url, bob, "bob"))
        compose.onNodeWithText("Withdraw").assertDoesNotExist()
    }

    @Test
    fun withdrawingEndsTheQuestion() {
        open(Session(server.url, alice, "alice"))
        compose.onNodeWithText("Withdraw").performClick()
        compose.awaitText("withdrawn", substring = true)
        compose.awaitMain {
            field(server.call("GET", "/exchanges/$id", bob), "state") == "withdrawn"
        }
    }

    /**
     * The screen may show the withdrawal from the live feed before the server answers the request
     * itself, so whether the request succeeded is asked of the account directly.
     */
    @Test
    fun theAccountSaysTheWithdrawalWorked() {
        app.sessions.keep(Session(server.url, alice, "alice"))
        assertTrue(runBlocking { app.account()!!.withdraw(id) })
    }

    @Test
    fun saysWhenTheServerCannotBeReached() {
        copied(NOWHERE, alice)
        compose.onNodeWithText("Withdraw").performClick()
        compose.awaitText("The question couldn't be withdrawn", substring = true)
    }

    @Test
    fun saysWhenTheServerRefuses() {
        copied(server.url, "transom_signed-out")
        compose.onNodeWithText("Withdraw").performClick()
        compose.awaitText("The question couldn't be withdrawn", substring = true)
    }
}
