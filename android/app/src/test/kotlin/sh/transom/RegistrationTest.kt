package sh.transom

import androidx.work.WorkManager
import java.io.IOException
import java.util.concurrent.atomic.AtomicInteger
import kotlin.time.Duration.Companion.milliseconds
import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.signin.Session
import sh.transom.signin.app
import sh.transom.sync.Backoff
import sh.transom.testserver.TestServer
import sh.transom.testserver.call

/**
 * The phone registers as a device once per account, trying again while the server is out of reach.
 */
@RunWith(RobolectricTestRunner::class)
class RegistrationTest {
    @get:Rule val server = TestServer()

    private val requests = AtomicInteger()
    private var account: Account? = null

    /**
     * The account of [session], over a network that fails its first [failing] requests as a lost
     * connection would.
     */
    private fun account(session: Session, failing: Int = 0): Account {
        val http =
            OkHttpClient.Builder()
                .addInterceptor { chain ->
                    if (requests.incrementAndGet() <= failing) throw IOException("offline")
                    chain.proceed(chain.request())
                }
                .build()
        val made =
            Account(
                session,
                app,
                WorkManager.getInstance(app),
                http,
                app.database,
                Backoff(first = 10.milliseconds),
            )
        account = made
        return made
    }

    @After
    fun forget() {
        runBlocking { account?.forget() }
    }

    @Test
    fun aRegistrationThatCouldNotBeSentIsTriedAgain() {
        val bob = server.tokenFor("bob")
        runBlocking { account(Session(server.url, bob, "bob"), failing = 2).register().join() }
        assertEquals(3, requests.get())
        assertTrue(server.call("GET", "/@bob/devices", bob).contains(""""connection":true"""))
    }

    @Test
    fun aRegistrationRefusedForGoodIsNotTriedAgain() {
        // Alice's token cannot register a device of Bob's.
        val alice = server.tokenFor("alice")
        server.tokenFor("bob")
        runBlocking { account(Session(server.url, alice, "bob")).register().join() }
        assertEquals(1, requests.get())
    }

    @Test
    fun theAccountRegistersOnceHoweverOftenItIsAsked() {
        val made = account(Session(server.url, server.tokenFor("bob"), "bob"))
        runBlocking {
            made.register().join()
            made.register().join()
        }
        assertEquals(1, requests.get())
    }
}
