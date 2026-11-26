package sh.transom

import android.app.Notification
import android.app.NotificationManager
import android.content.Context
import androidx.core.app.NotificationCompat
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import sh.transom.notifications.Channels
import sh.transom.notifications.await
import sh.transom.signin.Problem
import sh.transom.signin.Session
import sh.transom.signin.SignedOutNotification
import sh.transom.signin.app
import sh.transom.testserver.ENDED
import sh.transom.testserver.TestServer

/**
 * When a request learns that the server ended the session, the app signs out, drops what it had
 * queued, and posts one notification that says so.
 */
@RunWith(RobolectricTestRunner::class)
class SignedOutByServerTest {
    @get:Rule val server = TestServer()

    private val ended by lazy { Session(server.url, ENDED, "bob") }

    @After
    fun signOut() {
        runBlocking { app.signedOut() }
    }

    private val manager: NotificationManager
        get() = app.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

    private fun notice(): Notification? =
        shadowOf(manager).getNotification(SignedOutNotification.NOTIFICATION)

    /**
     * Signed in with the ended session, with [replies] queued, until a catch-up learns it ended.
     */
    private fun signedOut(replies: Int = 0): Notification {
        app.sessions.keep(ended)
        val account = app.account()!!
        runBlocking { repeat(replies) { account.outbox.reply("exchange-$it", text = "on my way") } }
        // As the account's own work, since signing out cancels what the account runs.
        account.background { account.catchUp() }
        await { notice() != null }
        return notice()!!
    }

    private val Notification.text: String?
        get() = extras.getCharSequence(NotificationCompat.EXTRA_TEXT)?.toString()

    @Test
    fun theSessionIsForgottenWithTheServersReason() {
        signedOut()
        assertNull(app.sessions.session())
        assertEquals(Problem.Ended::class, app.sessions.ended!!::class)
    }

    @Test
    fun theNotificationSaysTheAppSignedOut() {
        val shown = signedOut()
        assertEquals(
            listOf(
                "transom signed you out",
                "Sign in again to get your notifications.",
                Channels.DELIVERY,
            ),
            listOf(
                shown.extras.getString(NotificationCompat.EXTRA_TITLE),
                shown.text,
                shown.channelId,
            ),
        )
    }

    @Test
    fun itSaysAReplyWasNotSent() {
        assertEquals(
            "Sign in again to get your notifications. 1 reply wasn't sent.",
            signedOut(replies = 1).text,
        )
    }

    @Test
    fun itSaysHowManyRepliesWereNotSent() {
        assertEquals(
            "Sign in again to get your notifications. 2 replies weren't sent.",
            signedOut(replies = 2).text,
        )
    }

    @Test
    fun theQueueIsDropped() {
        signedOut(replies = 1)
        assertEquals(0, runBlocking { app.database.queue().replies().size })
    }

    @Test
    fun theOutboxsOwnDeliveryLearnsItToo() {
        app.sessions.keep(ended)
        val account = app.account()!!
        runBlocking { account.outbox.reply("an-exchange", text = "on my way") }
        account.background { account.deliver() }
        await { notice() != null }
        assertEquals(
            "Sign in again to get your notifications. 1 reply wasn't sent.",
            notice()!!.text,
        )
    }

    /**
     * A wake-up's work keeps the process alive only while it runs, so it returns once the app has
     * signed out and said so, not before.
     */
    @Test
    fun aCatchUpReturnsOnceTheAppHasSaidItSignedOut() {
        app.sessions.keep(ended)
        assertTrue(runBlocking { app.account()!!.catchUp() })
        assertNotNull(notice())
    }

    @Test
    fun aDeliveryReturnsOnceTheAppHasSaidItSignedOut() {
        app.sessions.keep(ended)
        val account = app.account()!!
        runBlocking { account.outbox.reply("an-exchange", text = "on my way") }
        assertTrue(runBlocking { account.deliver() })
        assertNotNull(notice())
    }

    @Test
    fun thePhoneKeepsTheServersAddress() {
        signedOut()
        assertEquals(server.url, app.preferences.server)
    }

    @Test
    fun signingInAgainTakesTheNotificationAway() {
        signedOut()
        app.sessions.keep(Session(server.url, server.tokenFor("bob"), "bob"))
        app.signedIn()
        assertNull(notice())
    }

    @Test
    fun aSessionAlreadyReplacedIsLeftSignedIn() {
        val now = Session(server.url, server.tokenFor("bob"), "bob")
        app.sessions.keep(now)
        runBlocking { app.endedBy(ended, "session_ended") }
        assertEquals(now, app.sessions.session())
    }
}
