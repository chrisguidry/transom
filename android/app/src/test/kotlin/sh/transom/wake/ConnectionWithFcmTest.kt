package sh.transom.wake

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import sh.transom.notifications.Connected
import sh.transom.notifications.ConnectionService
import sh.transom.notifications.await
import sh.transom.notifications.awaitPosted
import sh.transom.signin.Session
import sh.transom.signin.app
import sh.transom.testApp
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.send

/**
 * With FCM, the background connection ends once the server takes the token, and holds on when the
 * server refuses it.
 */
@RunWith(RobolectricTestRunner::class)
class ConnectionWithFcmTest {
    @get:Rule val server = TestServer()
    @get:Rule val connected = Connected()

    @Before
    fun fcm() {
        testApp.fcm = Push { REFUSED }
    }

    @Test
    fun theConnectionEndsOnceTheServerTakesTheToken() {
        testApp.fcm = Push { "token-1" }
        app.sessions.keep(Session(server.url, server.tokenFor("bob"), "bob"))
        val service = connected.start()
        await { shadowOf(service).isStoppedBySelf }
    }

    @Test
    fun theConnectionHoldsWhenTheServerRefusesTheToken() {
        val alice = server.tokenFor("alice")
        val bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
        val service = connected.signedIn(server, "bob")
        val id = server.send(alice, "bob", """{"summary": "Still there?"}""")
        awaitPosted(id)
        assertFalse(shadowOf(service).isStoppedBySelf)
    }

    @Test
    fun signingInStartsTheConnectionWhenTheServerRefusesTheToken() {
        app.sessions.keep(Session(server.url, server.tokenFor("bob"), "bob"))
        app.signedIn()
        await { shadowOf(app).peekNextStartedService() != null }
        assertEquals(
            ConnectionService::class.java.name,
            shadowOf(app).nextStartedService.component?.className,
        )
    }
}
