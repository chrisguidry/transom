package sh.transom.delivery

import android.os.Bundle
import androidx.work.WorkInfo
import androidx.work.WorkManager
import androidx.work.testing.WorkManagerTestInitHelper
import com.google.firebase.messaging.RemoteMessage
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import sh.transom.exchange.message
import sh.transom.notifications.await
import sh.transom.notifications.awaitCleared
import sh.transom.notifications.awaitPosted
import sh.transom.signin.Session
import sh.transom.signin.app
import sh.transom.testApp
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.send

/** @bob's phone, which FCM wakes, catches up with what @alice sends each time it is woken. */
@RunWith(RobolectricTestRunner::class)
class WakeUpTest {
    @get:Rule val server = TestServer()

    private lateinit var alice: String
    private lateinit var bob: String

    @Before
    fun bobIsWokenByFcm() {
        alice = server.tokenFor("alice")
        bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
        testApp.fcm(Push { "token-1" })
        app.sessions.keep(Session(server.url, bob, "bob"))
        app.signedIn()
        await { app.account()?.delivery?.value is DeliveryState.Delivering }
    }

    /** Signs out as SignIn.signOut does: the session is forgotten first. */
    @After
    fun signOut() {
        app.sessions.forget()
        runBlocking { app.signedOut() }
    }

    private fun devices(): String = server.call("GET", "/@bob/devices", bob)

    private val service: PushService
        get() = Robolectric.buildService(PushService::class.java).create().get()

    /** The wake-up's work, once it passes [condition]. */
    private fun awaitWork(condition: (WorkInfo) -> Boolean) = await {
        WorkManager.getInstance(app)
            .getWorkInfosForUniqueWork(CatchUpWorker.NAME)
            .get()
            .any(condition)
    }

    /** Wakes the phone as FCM does, with the phone online. */
    private fun wake() {
        service.onMessageReceived(RemoteMessage(Bundle()))
        awaitWork { it.state == WorkInfo.State.ENQUEUED }
        val work =
            WorkManager.getInstance(app)
                .getWorkInfosForUniqueWork(CatchUpWorker.NAME)
                .get()
                .single { it.state == WorkInfo.State.ENQUEUED }
        checkNotNull(WorkManagerTestInitHelper.getTestDriver(app)).setAllConstraintsMet(work.id)
    }

    @Test
    fun signingInLeavesTheConnectionStopped() {
        assertNull(shadowOf(app).nextStartedService)
    }

    @Test
    fun aWakeUpPostsTheQuestionThatArrived() {
        val id = server.send(alice, "bob", message("Deploy?", choices = listOf("Deploy" to null)))
        wake()
        awaitPosted(id)
    }

    @Test
    fun aOneWayNotificationThatArrivedBeforeTheWakeUpIsNews() {
        val id = server.send(alice, "bob", """{"summary": "Backup done"}""")
        wake()
        awaitPosted(id)
    }

    @Test
    fun aWakeUpClearsAQuestionThatWasWithdrawn() {
        val id = server.send(alice, "bob", message("Deploy?", choices = listOf("Deploy" to null)))
        wake()
        awaitPosted(id)
        server.call("DELETE", "/exchanges/$id/message", alice)
        wake()
        awaitCleared(id)
    }

    @Test
    fun aWakeUpAfterSigningOutDoesNothing() {
        signOut()
        wake()
        awaitWork { it.state == WorkInfo.State.SUCCEEDED }
    }

    @Test
    fun aNewTokenIsRegistered() {
        service.onRegistered("token-2")
        await { app.account()?.delivery?.value == DeliveryState.Delivering("token-2") }
        assertTrue(devices().contains(""""fcm":"token-2""""))
    }

    @Test
    fun aNewTokenTheServerRefusesFailsAndLeavesTheConnectionStopped() {
        service.onRegistered(REFUSED)
        await { app.account()?.delivery?.value == DeliveryState.Failing.Refused }
        assertNull(shadowOf(app).nextStartedService)
    }

    @Test
    fun aNewTokenAfterSigningOutIsNotRegistered() {
        signOut()
        service.onRegistered("token-2")
        assertFalse(devices().contains("token-2"))
    }
}
