package sh.transom.delivery

import androidx.work.WorkManager
import java.io.IOException
import java.util.concurrent.atomic.AtomicInteger
import kotlin.time.Duration.Companion.milliseconds
import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import org.junit.rules.ExternalResource
import sh.transom.Account
import sh.transom.signin.Session
import sh.transom.signin.app
import sh.transom.sync.Backoff
import sh.transom.testserver.TestServer
import sh.transom.testserver.call

/**
 * Bob's account on [server] for a test, counting its [requests]. After the test, it forgets the
 * account, so nothing it runs outlives the test.
 */
class BobsAccount(private val server: TestServer) : ExternalResource() {
    val requests = AtomicInteger()

    private var account: Account? = null

    private val bob: String by lazy { server.tokenFor("bob") }

    /**
     * Bob's account delivering through [push], or the connection when it is `null`, over a network
     * that fails its first [failing] requests as a lost connection would.
     */
    fun with(push: Push?, failing: Int = 0): Account {
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

    /** Bob's account delivering through [push], once its registration has settled. */
    fun registered(push: Push?): Account = with(push).also { runBlocking { it.register().join() } }

    /** Bob's devices, as the server lists them. */
    fun devices(): String = server.call("GET", "/@bob/devices", bob)

    override fun after() {
        runBlocking { account?.forget() }
    }
}
