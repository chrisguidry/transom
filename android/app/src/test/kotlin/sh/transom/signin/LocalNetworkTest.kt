package sh.transom.signin

import java.io.IOException
import java.net.InetAddress
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf

/** Which addresses the internet routes to, so a server there needs no local network permission. */
@RunWith(Parameterized::class)
class PublicAddressTest(private val address: String, private val public: Boolean) {
    @Test
    fun theInternetRoutesOnlyToPublicAddresses() {
        assertEquals(public, InetAddress.getByName(address).isPublic())
    }

    companion object {
        @JvmStatic
        @Parameterized.Parameters(name = "{0}")
        fun cases() =
            listOf(
                arrayOf<Any>("10.0.0.240", false),
                arrayOf<Any>("172.16.0.1", false),
                arrayOf<Any>("192.168.1.2", false),
                arrayOf<Any>("169.254.0.1", false),
                arrayOf<Any>("127.0.0.1", false),
                arrayOf<Any>("0.0.0.0", false),
                arrayOf<Any>("224.0.0.251", false),
                arrayOf<Any>("fd12:3456::1", false),
                arrayOf<Any>("fe80::1", false),
                arrayOf<Any>("::1", false),
                arrayOf<Any>("93.184.216.34", true),
                arrayOf<Any>("2606:4700::1111", true),
            )
    }
}

/** Whether a server's name puts it on the local network. */
class LocalNetworkTest {
    @Test
    fun aServerAtAPrivateAddressIsOnTheLocalNetwork() {
        assertTrue(onLocalNetwork("10.0.0.240"))
    }

    @Test
    fun aServerAtAPublicAddressIsNot() {
        assertFalse(onLocalNetwork("93.184.216.34"))
    }

    @Test
    fun aNameThatDoesNotResolveIsNot() {
        assertFalse(onLocalNetwork("nowhere.invalid"))
    }
}

/** When the app asks for the local network permission, by the phone's Android and the problem. */
@RunWith(RobolectricTestRunner::class)
class AsksForLocalNetworkTest {
    private val local = Problem.Unreachable(IOException(), local = true)

    @Test
    fun asksForAServerOnTheLocalNetwork() {
        assertTrue(app.asksForLocalNetwork(local, sdk = 37))
    }

    @Test
    fun neverAsksBeforeAndroid17() {
        assertFalse(app.asksForLocalNetwork(local, sdk = 36))
    }

    @Test
    fun neverAsksForAServerAtAPublicAddress() {
        assertFalse(app.asksForLocalNetwork(Problem.Unreachable(IOException(), local = false)))
    }

    @Test
    fun neverAsksForAServerThatAnswered() {
        assertFalse(app.asksForLocalNetwork(Problem.NotTransom))
    }

    @Test
    fun neverAsksOnceAllowed() {
        shadowOf(app).grantPermissions(LOCAL_NETWORK)
        assertFalse(app.asksForLocalNetwork(local))
    }
}
