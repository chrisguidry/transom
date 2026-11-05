package sh.transom.delivery

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.signin.app
import sh.transom.testserver.TestServer

/**
 * The phone registers as the device its delivery makes it: with FCM's token for Google Play
 * services, which fails visibly rather than falling back, or as a phone that holds a connection.
 */
@RunWith(RobolectricTestRunner::class)
class RegisteringTest {
    @get:Rule val server = TestServer()
    @get:Rule val bob = BobsAccount(server)

    /** How many devices Bob has. */
    private fun deviceCount(): Int = Regex(""""id":""").findAll(bob.devices()).count()

    @Test
    fun withoutPushThePhoneHoldsAConnectionFromTheStart() {
        assertEquals(DeliveryState.Connection, bob.with(push = null).delivery.value)
    }

    @Test
    fun theConnectionRegistersThePhoneAsADeviceThatHoldsTheFeedOpen() {
        bob.registered(push = null)
        assertTrue(bob.devices().contains(""""wake":{"connection":true}"""))
    }

    @Test
    fun withPushItIsRegisteringUntilItSettles() {
        assertEquals(DeliveryState.Registering, bob.with(Push { "token-1" }).delivery.value)
    }

    @Test
    fun aTokenTheServerTakesDeliversThroughIt() {
        val made = bob.registered(Push { "token-1" })
        assertEquals(DeliveryState.Delivering("token-1"), made.delivery.value)
        assertTrue(bob.devices().contains(""""wake":{"fcm":"token-1"}"""))
    }

    @Test
    fun aServerThatRefusesTheTokenFailsWithoutFallingBack() {
        val made = bob.registered(Push { REFUSED })
        assertEquals(DeliveryState.Failing.Refused, made.delivery.value)
        assertEquals(1, bob.requests.get())
        assertEquals(0, deviceCount())
    }

    @Test
    fun noTokenFromFcmFailsWithoutFallingBack() {
        val made = bob.registered(Push { null })
        assertEquals(DeliveryState.Failing.NoToken, made.delivery.value)
        assertEquals(0, deviceCount())
    }

    @Test
    fun aTokenThatCouldNotBeSentIsTriedAgain() {
        val made = bob.with(Push { "token-1" }, failing = 2)
        runBlocking { made.register().join() }
        assertEquals(DeliveryState.Delivering("token-1"), made.delivery.value)
        assertEquals(3, bob.requests.get())
    }

    @Test
    fun takingTheTokenBeginsNotifying() {
        val before = System.currentTimeMillis()
        bob.registered(Push { "token-1" })
        assertTrue(runBlocking { app.database.notified().kept() } >= before)
    }

    @Test
    fun aNewTokenIsRegisteredInPlaceOfTheOld() {
        val made = bob.registered(Push { "token-1" })
        runBlocking { made.registerToken("token-2")?.join() }
        assertEquals(DeliveryState.Delivering("token-2"), made.delivery.value)
        assertTrue(bob.devices().contains(""""fcm":"token-2""""))
    }

    @Test
    fun aNewTokenIsNotTheServersBusinessWithTheConnection() {
        val made = bob.registered(push = null)
        assertNull(made.registerToken("token-2"))
        assertFalse(bob.devices().contains("token-2"))
    }

    @Test
    fun switchingToTheConnectionRegistersTheSameDeviceAgain() {
        val made = bob.registered(Push { "token-1" })
        runBlocking { made.deliverBy(null).join() }
        assertEquals(DeliveryState.Connection, made.delivery.value)
        assertTrue(bob.devices().contains(""""wake":{"connection":true}"""))
        assertEquals(1, deviceCount())
    }

    @Test
    fun switchingToGooglePlayRegistersTheTokenForTheSameDevice() {
        val made = bob.registered(push = null)
        runBlocking { made.deliverBy(Push { "token-1" }).join() }
        assertEquals(DeliveryState.Delivering("token-1"), made.delivery.value)
        assertTrue(bob.devices().contains(""""wake":{"fcm":"token-1"}"""))
        assertEquals(1, deviceCount())
    }
}
