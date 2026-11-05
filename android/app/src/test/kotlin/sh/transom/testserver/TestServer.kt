package sh.transom.testserver

import java.net.URI
import java.net.URLEncoder
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse
import java.time.Duration
import java.util.UUID
import org.junit.rules.ExternalResource

/**
 * A real transom for a test, with a real `test-issuer` it trusts and the Redis at
 * `TRANSOM_TEST_REDIS_URL`, so a test talks to the server's real API with no fakes in between.
 *
 * ```
 * class ExchangesTest {
 *     @get:Rule val server = TestServer()
 *
 *     @Test
 *     fun listsWhatWasSent() {
 *         val alice = server.tokenFor("alice")
 *         // Requests to "${server.url}/@alice/exchanges" with "Authorization: Bearer $alice".
 *     }
 * }
 * ```
 *
 * Use it as a [org.junit.Rule], so each test has a server of its own. transom's Redis prefix is a
 * setting of the server, and a fresh server is what gives each test a prefix, and so keys, no other
 * test sees: every test can be `@alice` from her first request. A test that starts both processes,
 * signs the app in, and stops them takes about 130 ms, so a server per test costs the suite little.
 * As a [org.junit.ClassRule] it works too, with the tests of the class sharing one server and its
 * keys.
 *
 * Gradle builds both binaries before the tests run and names them in the system properties
 * `transom.binary` and `test-issuer.binary`, with the Redis URL in `transom.test.redis.url`.
 */
class TestServer : ExternalResource() {
    /** The server's base URL, such as `http://127.0.0.1:41623`, with no trailing slash. */
    lateinit var url: String
        private set

    private lateinit var issuerUrl: String
    private lateinit var issuer: Spawned
    private var transom: Spawned? = null
    private val redisUrl = property("transom.test.redis.url")

    internal val http: HttpClient =
        HttpClient.newBuilder()
            .followRedirects(HttpClient.Redirect.NEVER)
            .connectTimeout(TIMEOUT)
            .build()

    override fun before() {
        issuer =
            spawn(
                listOf(
                    property("test-issuer.binary"),
                    "--audience",
                    AUDIENCE,
                    "--client-id",
                    CLIENT_ID,
                    "--client-secret",
                    CLIENT_SECRET,
                ),
                environment = emptyMap(),
                Watched.STDOUT,
            ) {
                // The issuer prints its URL once it is listening, and nothing before it.
                it.trim()
            }
        try {
            issuerUrl = issuer.awaitReady(STARTUP_SECONDS)
            url = startTransom()
        } catch (failure: Throwable) {
            after()
            throw failure
        }
    }

    private fun startTransom(): String {
        val issuerSettings =
            mapOf(
                "URL" to issuerUrl,
                "AUDIENCE" to AUDIENCE,
                "NAME_CLAIM" to "preferred_username",
                "CLIENT_ID" to CLIENT_ID,
                "CLIENT_SECRET" to CLIENT_SECRET,
                "LABEL" to "Test issuer",
            )
        val environment =
            mapOf(
                "TRANSOM_LISTEN" to "127.0.0.1:0",
                "TRANSOM_REDIS_URL" to redisUrl,
                "TRANSOM_REDIS_PREFIX" to "android-test-${UUID.randomUUID()}",
                "RUST_LOG" to "info",
                "NO_COLOR" to "1",
            ) + issuerSettings.mapKeys { "TRANSOM_ISSUERS__${ISSUER.uppercase()}__${it.key}" }
        val started =
            spawn(listOf(property("transom.binary"), "serve"), environment, Watched.STDERR) {
                listening.find(it)?.groupValues?.get(1)
            }
        transom = started
        // transom logs its address once it is listening, and the health check then answers
        // whether it reaches Redis, which it only connects to when a request needs it.
        val base = "http://${started.awaitReady(STARTUP_SECONDS)}"
        val health = http.send(get("$base/healthz").build(), HttpResponse.BodyHandlers.ofString())
        check(health.statusCode() == 200) {
            "transom cannot reach the Redis at $redisUrl, which the tests need: start one there, " +
                "or set TRANSOM_TEST_REDIS_URL to one that runs.\n${started.log()}"
        }
        return base
    }

    override fun after() {
        transom?.let {
            it.stop()
            // The server's log, one line per request, goes to the test's standard output, which
            // Gradle keeps in the test's report.
            println(it.log())
        }
        issuer.stop()
    }

    /**
     * An access token from the test issuer for the person [name], whose subject is also [name], so
     * a test acts as `@alice` with `Authorization: Bearer ${tokenFor("alice")}`.
     *
     * `@name` exists once it returns, so others can send to it at once: transom creates a person at
     * their first `GET /@{name}` with their token, and this makes that request.
     */
    fun tokenFor(name: String): String {
        val token =
            http.send(
                get("$issuerUrl/access-token?subject=${encoded(name)}&name=${encoded(name)}")
                    .build(),
                HttpResponse.BodyHandlers.ofString(),
            )
        check(token.statusCode() == 200) { "the issuer gave no token: ${token.body()}" }
        val person =
            http.send(
                get("$url/@${encoded(name)}")
                    .header("Authorization", "Bearer ${token.body()}")
                    .build(),
                HttpResponse.BodyHandlers.ofString(),
            )
        check(person.statusCode() == 200) { "transom did not create @$name: ${person.body()}" }
        return token.body()
    }

    /**
     * Signs [name] in as the app does, with the S256 [challenge] of a PKCE verifier, and returns
     * the one-time code the server sends to [APP_CALLBACK]. Redeem it with `POST /login/app`.
     *
     * The sign-in alone does not create `@name`: the person exists after the app's first `GET
     * /@{name}` with its session, as for a token from [tokenFor].
     */
    fun signInApp(name: String, challenge: String): String = appSignIn(this, name, challenge)

    internal fun get(url: String): HttpRequest.Builder =
        HttpRequest.newBuilder(URI.create(url)).timeout(TIMEOUT).GET()

    companion object {
        /** The app's App Link, which the server sends sign-in codes to by default. */
        const val APP_CALLBACK = "https://transom.sh/app/callback"

        /** The issuer's name in transom's settings, which the sign-in page links to. */
        internal const val ISSUER = "test"

        private const val AUDIENCE = "transom-tests"
        private const val CLIENT_ID = "transom"
        private const val CLIENT_SECRET = "transom-test-secret"
        private const val STARTUP_SECONDS = 10L
        private val TIMEOUT: Duration = Duration.ofSeconds(10)

        private val listening = Regex("""listening address=(\S+)""")

        private fun property(name: String): String =
            checkNotNull(System.getProperty(name)) {
                "the system property $name is not set: run the tests with Gradle, which sets it"
            }

        internal fun encoded(value: String): String = URLEncoder.encode(value, Charsets.UTF_8)
    }
}
