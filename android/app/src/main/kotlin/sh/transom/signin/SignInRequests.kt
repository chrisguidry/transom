package sh.transom.signin

import java.io.IOException
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response

/** Where the server sends the Custom Tab with the one-time code: the App Link the app claims. */
const val APP_CALLBACK = "https://transom.sh/app/callback"

/** What went wrong with a sign-in, as the sign-in screen tells the person. */
enum class Problem {
    /** What they typed is no URL. */
    NotAnAddress,
    /** The server did not answer, or the phone is offline. */
    Unreachable,
    /** Something answered that is not a transom server. */
    NotTransom,
    /** The Custom Tab closed before the sign-in finished. */
    Cancelled,
    /** The server refused the one-time code, or the session it gave. */
    Refused,
    /** No browser is installed to sign in with. */
    NoBrowser,
}

internal class SignInFailed(val problem: Problem, cause: Throwable? = null) :
    Exception(problem.name, cause)

/**
 * The calls signing in and out make to the server. Each blocks, and throws [SignInFailed] with the
 * problem the person is shown.
 */
class SignInRequests(http: OkHttpClient) {
    // The server answers GET / with a redirect to the person's own inbox, which carries their name,
    // so the redirect is read rather than followed.
    private val http = http.newBuilder().followRedirects(false).build()

    /** The page the Custom Tab opens to begin signing in, with the app's PKCE [challenge]. */
    fun loginUrl(server: String, challenge: String): HttpUrl =
        "$server/login"
            .toHttpUrl()
            .newBuilder()
            .addQueryParameter("app", challenge)
            .addQueryParameter("callback", APP_CALLBACK)
            .build()

    /**
     * Asks [server] for the sign-in page the Custom Tab is about to open, so a mistyped address is
     * told before a browser opens. transom's page carries the app's [challenge] on its link to each
     * issuer, and nothing but transom would.
     */
    internal fun check(server: String, challenge: String) {
        val request =
            Request.Builder().url(loginUrl(server, challenge)).header("Accept", "text/html").build()
        call(request) { response ->
            if (!response.isSuccessful || challenge !in response.body.string()) {
                throw SignInFailed(Problem.NotTransom)
            }
        }
    }

    /** Trades the one-time [code] and the [verifier] behind its challenge for a session's token. */
    internal fun redeem(server: String, code: String, verifier: String): String {
        val body = Json.encodeToString(Redemption.serializer(), Redemption(code, verifier))
        val request =
            Request.Builder()
                .url("$server/login/app")
                .post(body.toRequestBody("application/json".toMediaType()))
                .build()
        return call(request) { response ->
            when {
                response.code == 400 -> throw SignInFailed(Problem.Refused)
                !response.isSuccessful -> throw SignInFailed(Problem.NotTransom)
                else -> json.decodeFromString(Redeemed.serializer(), response.body.string()).token
            }
        }
    }

    /**
     * The `@name` that [token] signs in as. transom answers `GET /` with a `303` to
     * `/@{name}/exchanges`.
     */
    internal fun nameOf(server: String, token: String): String {
        val request =
            Request.Builder().url("$server/").header("Authorization", "Bearer $token").build()
        return call(request) { response ->
            if (response.code == 401) throw SignInFailed(Problem.Refused)
            val location = response.header("Location")
            val segments = location?.let { "$server/".toHttpUrl().resolve(it) }?.pathSegments
            val name = segments?.firstOrNull()?.removePrefix("@")
            if (
                response.code != 303 ||
                    segments?.getOrNull(1) != "exchanges" ||
                    name.isNullOrEmpty()
            ) {
                throw SignInFailed(Problem.NotTransom)
            }
            name
        }
    }

    /**
     * Ends [session] on its server. The app forgets the session whatever the server answers, so
     * this tells the caller nothing.
     */
    fun signOut(session: Session) {
        val request =
            Request.Builder()
                .url("${session.server}/logout")
                .header("Authorization", "Bearer ${session.token}")
                .post(ByteArray(0).toRequestBody())
                .build()
        try {
            http.newCall(request).execute().close()
        } catch (_: IOException) {}
    }

    private fun <T> call(request: Request, read: (Response) -> T): T =
        try {
            http.newCall(request).execute().use(read)
        } catch (failure: IOException) {
            throw SignInFailed(Problem.Unreachable, failure)
        } catch (failure: SerializationException) {
            throw SignInFailed(Problem.NotTransom, failure)
        }
}

@Serializable private class Redemption(val code: String, val verifier: String)

@Serializable private class Redeemed(val token: String)

private val json = Json { ignoreUnknownKeys = true }
