package sh.transom

import androidx.work.WorkManager
import java.util.concurrent.atomic.AtomicInteger
import kotlin.time.Duration.Companion.milliseconds
import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.signin.Session
import sh.transom.signin.SignInRequests
import sh.transom.signin.app
import sh.transom.sync.Backoff
import sh.transom.sync.SignedOut
import sh.transom.testserver.ENDED
import sh.transom.testserver.TestServer
import sh.transom.testserver.call
import sh.transom.testserver.sessionFor

/**
 * An account whose session the server ended reports it once, the first time a request hears it, and
 * tries nothing again; while it signs out on purpose, it reports nothing.
 */
@RunWith(RobolectricTestRunner::class)
class EndedSessionTest {
    @get:Rule val server = TestServer()

    private val requests = AtomicInteger()

    /** Each code the account reported a session's end with. */
    private val heard = mutableListOf<String?>()

    private var account: Account? = null

    /** The account of [token] for [name], counting its requests, over [http]. */
    private fun account(
        token: String = ENDED,
        name: String = "bob",
        http: OkHttpClient.Builder = OkHttpClient.Builder(),
    ): Account {
        val counted =
            http
                .addInterceptor { chain ->
                    requests.incrementAndGet()
                    chain.proceed(chain.request())
                }
                .build()
        val made =
            Account(
                Session(server.url, token, name),
                app,
                WorkManager.getInstance(app),
                counted,
                app.database,
                Backoff(first = 10.milliseconds),
            ) {
                heard += it
                null
            }
        account = made
        return made
    }

    @After
    fun forget() {
        runBlocking { account?.forget() }
    }

    @Test
    fun aCatchUpStopsWithoutAskingAgain() {
        assertTrue(runBlocking { account().catchUp() })
        assertEquals(1, heard.size)
    }

    @Test
    fun itIsReportedOnceHoweverManyRequestsHearIt() {
        val made = account()
        runBlocking {
            made.catchUp()
            made.catchUp()
        }
        assertEquals(2, requests.get())
        assertEquals(1, heard.size)
    }

    @Test
    fun aRegistrationIsNotTriedAgain() {
        runBlocking { account().register().join() }
        assertEquals(1, requests.get())
        assertEquals(1, heard.size)
    }

    @Test
    fun theOutboxIsNotSentAgain() {
        val made = account()
        runBlocking { made.outbox.reply("an-exchange", text = "on my way") }
        assertTrue(runBlocking { made.deliver() })
        assertEquals(1, heard.size)
    }

    @Test
    fun aWithdrawalSaysItDidNotWork() {
        assertFalse(runBlocking { account().withdraw("an-exchange") })
    }

    /** A proxy in front of the server that answers every request with a 401 of its own. */
    private val proxy =
        OkHttpClient.Builder().addInterceptor { chain ->
            Response.Builder()
                .request(chain.request())
                .protocol(Protocol.HTTP_1_1)
                .code(401)
                .message("Unauthorized")
                .body("<h1>Sign in to the proxy</h1>".toResponseBody())
                .build()
        }

    @Test
    fun aCatchUpThatAProxyRefusesIsTriedAgainLater() {
        assertFalse(runBlocking { account(http = proxy).catchUp() })
        assertEquals(0, heard.size)
    }

    @Test
    fun forgettingThePhoneRemovesItsDevice() {
        val bob = server.tokenFor("bob")
        val made = account(token = bob)
        runBlocking {
            made.register().join()
            made.deleteDevice()
        }
        assertFalse(server.call("GET", "/@bob/devices", bob).contains(""""connection":true"""))
    }

    @Test
    fun signingOutOnPurposeReportsNothing() {
        val token = server.sessionFor("alice")
        val made = account(token = token, name = "alice")
        runBlocking {
            made.register().join()
            made.deleteDevice()
            SignInRequests(OkHttpClient()).signOut(Session(server.url, token, "alice"))
            assertTrue(
                runCatching { made.client.exchanges("alice") }.exceptionOrNull() is SignedOut,
            )
        }
        assertEquals(0, heard.size)
    }
}
