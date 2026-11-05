package sh.transom.notifications

import android.content.Intent
import androidx.core.app.NotificationCompat
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import sh.transom.signin.Session
import sh.transom.signin.app
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.send

/** The background connection runs while the app is signed in, and only then. */
@RunWith(RobolectricTestRunner::class)
class ConnectionServiceTest {
    @get:Rule val server = TestServer()
    @get:Rule val connected = Connected()

    @Test
    fun itShowsAQuietNotificationWhileItRuns() {
        val service = connected.signedIn(server, "bob")
        val notification = shadowOf(service).lastForegroundNotification
        assertEquals(Channels.CONNECTION, notification.channelId)
        assertEquals(
            "Connected to 127.0.0.1",
            notification.extras.getString(NotificationCompat.EXTRA_TITLE),
        )
    }

    @Test
    fun itRegistersThePhoneAsADeviceThatHoldsTheFeedOpen() {
        connected.signedIn(server, "bob")
        val bob = server.tokenFor("bob")
        await {
            server.call("GET", "/@bob/devices", bob).contains(""""wake":{"connection":true}""")
        }
    }

    @Test
    fun itStopsWhenTheAppSignsOut() {
        val service = connected.signedIn(server, "bob")
        // Signing out forgets the session first, as SignIn.signOut does.
        app.sessions.forget()
        runBlocking { app.signedOut() }
        await { shadowOf(service).isStoppedBySelf }
    }

    @Test
    fun itStopsAtOnceWhenNoOneIsSignedIn() {
        val service = connected.start()
        assertTrue(shadowOf(service).isStoppedBySelf)
    }

    @Test fun nothingCanBindToIt() = assertNull(connected.start().onBind(Intent()))

    @Test
    fun signingOutAsksItToStop() {
        connected.signedIn(server, "bob")
        runBlocking { app.signedOut() }
        assertEquals(
            ConnectionService::class.java.name,
            shadowOf(app).nextStoppedService.component?.className,
        )
    }

    private fun boot(action: String) {
        BootReceiver().onReceive(app, Intent(action))
    }

    @Test
    fun theBootStartsItWhileSignedIn() {
        app.sessions.keep(Session(server.url, server.tokenFor("bob"), "bob"))
        boot(Intent.ACTION_BOOT_COMPLETED)
        assertEquals(
            ConnectionService::class.java.name,
            shadowOf(app).nextStartedService.component?.className,
        )
    }

    @Test
    fun anUpdateStartsItWhileSignedIn() {
        app.sessions.keep(Session(server.url, server.tokenFor("bob"), "bob"))
        boot(Intent.ACTION_MY_PACKAGE_REPLACED)
        assertEquals(
            ConnectionService::class.java.name,
            shadowOf(app).nextStartedService.component?.className,
        )
    }

    @Test
    fun theBootLeavesItStoppedWhileSignedOut() {
        boot(Intent.ACTION_BOOT_COMPLETED)
        assertNull(shadowOf(app).nextStartedService)
    }

    @Test
    fun otherBroadcastsLeaveItAlone() {
        app.sessions.keep(Session(server.url, server.tokenFor("bob"), "bob"))
        boot(Intent.ACTION_TIME_CHANGED)
        assertNull(shadowOf(app).nextStartedService)
    }

    @Test
    fun signingInLeavesItStoppedWhileItIsTurnedOff() {
        app.preferences.backgroundConnection = false
        app.sessions.keep(Session(server.url, server.tokenFor("bob"), "bob"))
        app.signedIn()
        assertNull(shadowOf(app).nextStartedService)
    }

    @Test
    fun theBootLeavesItStoppedWhileItIsTurnedOff() {
        app.preferences.backgroundConnection = false
        app.sessions.keep(Session(server.url, server.tokenFor("bob"), "bob"))
        boot(Intent.ACTION_BOOT_COMPLETED)
        assertNull(shadowOf(app).nextStartedService)
    }

    @Test
    fun aRenameMovesItToTheNewName() {
        val alice = server.tokenFor("alice")
        val bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
        val service = connected.signedIn(server, "bob")
        server.call("PATCH", "/@bob", bob, """{"name": "robert"}""")
        app.sessions.keep(Session(server.url, bob, "robert"))
        app.restarted()
        connected.startAgain()
        val id = server.send(alice, "robert", """{"summary": "Still there?"}""")
        awaitPosted(id)
        assertFalse(shadowOf(service).isStoppedBySelf)
    }
}
