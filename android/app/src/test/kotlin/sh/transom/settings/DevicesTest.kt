package sh.transom.settings

import android.os.Build
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import java.math.BigInteger
import java.security.KeyPairGenerator
import java.security.interfaces.ECPublicKey
import java.security.spec.ECGenParameterSpec
import java.util.Base64
import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.LaunchedApp
import sh.transom.exchange.TransomJson
import sh.transom.signin.Problem
import sh.transom.signin.Session
import sh.transom.signin.SignInFailed
import sh.transom.signin.SignInRequests
import sh.transom.signin.app
import sh.transom.signin.awaitHeading
import sh.transom.signin.awaitMain
import sh.transom.signin.awaitText
import sh.transom.signin.challengeOf
import sh.transom.signin.newVerifier
import sh.transom.testserver.NOWHERE
import sh.transom.testserver.TestServer
import sh.transom.testserver.call

/** @alice's devices, from Settings, against a real transom. */
@RunWith(RobolectricTestRunner::class)
class DevicesTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val server = TestServer()
    @get:Rule val launched = LaunchedApp()

    private lateinit var alice: String

    @Before
    fun people() {
        alice = server.tokenFor("alice")
    }

    private fun devices(): String = server.call("GET", "/@alice/devices", alice)

    /** Puts a device for @alice, holding [token], as another phone or browser would. */
    private fun device(id: String, body: String, token: String = alice) {
        server.call("PUT", "/@alice/devices/$id", token, body, expected = 201)
    }

    /** Opens the devices once this phone has registered as one of them. */
    private fun openDevices() {
        launched.signedIn(Session(server.url, alice, "alice"))
        compose.awaitMain { Build.MODEL in devices() }
        compose.openFromSettings("Your devices", "Devices")
    }

    @Test
    fun thisPhoneIsMarked() {
        openDevices()
        compose.awaitText("This phone · Holds a connection open")
    }

    @Test
    fun anotherPhoneSaysHowItWakes() {
        device("pixel", """{"name": "Pixel 9", "wake": {"connection": true}}""")
        openDevices()
        compose.awaitText("Pixel 9")
        compose.onNodeWithText("Holds a connection open").assertExists()
    }

    @Test
    fun aBrowserSaysItWakesByWebPush() {
        device("firefox", """{"name": "Firefox", "wake": {"web_push": ${subscription()}}}""")
        openDevices()
        compose.awaitText("A browser, woken by Web Push")
    }

    @Test
    fun aPhoneThatGooglePlayServicesDeliversToSaysSo() {
        val device =
            TransomJson.decodeFromString<Device>(
                """{"id": "pixel", "name": "Pixel 9", "wake": {"fcm": "token-1"}}"""
            )
        assertEquals("Google Play services", app.getString(device.deliveredBy.label))
    }

    @Test
    fun forgettingAnotherDeviceRemovesIt() {
        device("pixel", """{"name": "Pixel 9", "wake": {"connection": true}}""")
        openDevices()
        compose.awaitText("Pixel 9")
        compose.press("Forget")
        compose.awaitMain { compose.onAllNodes(hasText("Pixel 9")).fetchSemanticsNodes().isEmpty() }
        assertFalse("Pixel 9" in devices())
    }

    @Test
    fun forgettingAnotherPhoneSignsItOut() {
        val requests = SignInRequests(OkHttpClient())
        val verifier = newVerifier()
        val phone =
            requests.redeem(server.url, server.signInApp("alice", challengeOf(verifier)), verifier)
        device("pixel", """{"name": "Pixel 9", "wake": {"connection": true}}""", token = phone)
        openDevices()
        compose.awaitText("Pixel 9")
        compose.press("Forget")
        compose.awaitMain {
            runCatching { requests.nameOf(server.url, phone) }
                .exceptionOrNull()
                .let { it is SignInFailed && it.problem == Problem.Refused }
        }
    }

    @Test
    fun thisPhoneOffersSignOutInPlaceOfForget() {
        openDevices()
        compose.awaitText("This phone · Holds a connection open")
        compose.press("Sign out")
        compose.awaitHeading("Sign in")
    }

    @Test
    fun anUnreachableServerIsSaidPlainly() {
        launched.signedIn(Session(NOWHERE, "transom_token", "alice"))
        compose.openFromSettings("Your devices", "Devices")
        compose.awaitText("The server can't be reached. Check your connection and try again.")
    }

    /**
     * A browser's push subscription as JSON, with a real P-256 key, since the server refuses a
     * subscription it could not encrypt to.
     */
    private fun subscription(): String {
        val pair =
            KeyPairGenerator.getInstance("EC")
                .apply { initialize(ECGenParameterSpec("secp256r1")) }
                .generateKeyPair()
        val point = (pair.public as ECPublicKey).w
        val key = byteArrayOf(4) + point.affineX.octets() + point.affineY.octets()
        val encoder = Base64.getUrlEncoder().withoutPadding()
        return """{"endpoint": "https://push.example.com/send/1",
            "keys": {"p256dh": "${encoder.encodeToString(key)}",
            "auth": "${encoder.encodeToString(ByteArray(16))}"}}"""
    }

    /** The coordinate as 32 octets, as an uncompressed point holds it. */
    private fun BigInteger.octets(): ByteArray =
        toByteArray().takeLast(32).toByteArray().let { ByteArray(32 - it.size) + it }
}
