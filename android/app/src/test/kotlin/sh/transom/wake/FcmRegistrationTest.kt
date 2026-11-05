package sh.transom.wake

import androidx.work.WorkManager
import java.io.IOException
import java.util.concurrent.atomic.AtomicInteger
import kotlin.time.Duration.Companion.milliseconds
import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.Account
import sh.transom.signin.Session
import sh.transom.signin.app
import sh.transom.sync.Backoff
import sh.transom.sync.Wake
import sh.transom.testserver.TestServer
import sh.transom.testserver.call

/** With FCM, the phone registers its token, and holds its connection when the server refuses it. */
@RunWith(RobolectricTestRunner::class)
class FcmRegistrationTest {
    @get:Rule val server = TestServer()

    private val requests = AtomicInteger()
    private var account: Account? = null
    private lateinit var bob: String

    @Before
    fun people() {
        bob = server.tokenFor("bob")
    }

    /**
     * Bob's account with [push], over a network that fails its first [failing] requests as a lost
     * connection would.
     */
    private fun account(push: Push?, failing: Int = 0): Account {
        val http =
            OkHttpClient.Builder()
                .addInterceptor { chain ->
                    if (requests.incrementAndGet() <= failing) throw IOException("offline")
                    chain.proceed(chain.request())
                }
                .build()
        val made =
            Account(
                Session(server.url, bob, "bob"),
                app,
                WorkManager.getInstance(app),
                http,
                app.database,
                Backoff(first = 10.milliseconds),
                push,
            )
        account = made
        return made
    }

    @After
    fun forget() {
        runBlocking { account?.forget() }
    }

    private fun devices(): String = server.call("GET", "/@bob/devices", bob)

    private fun Account.registered(): Account = also { runBlocking { register().join() } }

    @Test
    fun withoutFcmThePhoneHoldsAConnectionFromTheStart() {
        assertEquals(Wake.Connection, account(push = null).wakes.value)
    }

    @Test
    fun withFcmHowItWakesIsUnknownUntilItRegisters() {
        assertNull(account(Push { "token-1" }).wakes.value)
    }

    @Test
    fun aServerThatRefusesTheTokenGetsAPhoneThatHoldsAConnection() {
        val made = account(Push { REFUSED }).registered()
        assertEquals(Wake.Connection, made.wakes.value)
        assertEquals(2, requests.get())
        assertTrue(devices().contains(""""wake":{"connection":true}"""))
    }

    @Test
    fun aTokenTheServerTakesWakesThePhoneByFcm() {
        val made = account(Push { "token-1" }).registered()
        assertEquals(Wake.Fcm("token-1"), made.wakes.value)
        assertTrue(devices().contains(""""wake":{"fcm":"token-1"}"""))
    }

    @Test
    fun aTokenThatCouldNotBeSentIsTriedAgain() {
        val made = account(Push { "token-1" }, failing = 2).registered()
        assertEquals(Wake.Fcm("token-1"), made.wakes.value)
        assertEquals(3, requests.get())
    }

    @Test
    fun takingTheTokenBeginsNotifying() {
        val before = System.currentTimeMillis()
        account(Push { "token-1" }).registered()
        assertTrue(runBlocking { app.database.notified().kept() } >= before)
    }

    @Test
    fun noTokenFromFcmGetsAPhoneThatHoldsAConnection() {
        val made = account(Push { null }).registered()
        assertEquals(Wake.Connection, made.wakes.value)
        assertTrue(devices().contains(""""wake":{"connection":true}"""))
    }

    @Test
    fun aNewTokenIsRegisteredInPlaceOfTheOld() {
        val made = account(Push { "token-1" }).registered()
        runBlocking { made.registerToken("token-2").join() }
        assertEquals(Wake.Fcm("token-2"), made.wakes.value)
        assertTrue(devices().contains(""""fcm":"token-2""""))
    }
}
