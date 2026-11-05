package sh.transom.delivery

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test

/**
 * Google Play services can deliver only in a build with all of the Firebase project's values, on a
 * phone with Google Play services, and Firebase starts only once the app asks for FCM.
 */
class GooglePlayForTest {
    private val values = FirebaseValues("transom-sh", "1:123:android:abc", "AIzaKey", "123")

    private val pushed = Push { "token-1" }

    @Test
    fun aBuildWithEveryValueHasThem() {
        assertEquals(values, FirebaseValues.of("transom-sh", "1:123:android:abc", "AIzaKey", "123"))
    }

    @Test
    fun aBuildWithoutValuesHasNone() {
        assertNull(FirebaseValues.of("", "", "", ""))
    }

    @Test
    fun aBuildMissingOneValueHasNone() {
        assertNull(FirebaseValues.of("transom-sh", "1:123:android:abc", "", "123"))
    }

    @Test
    fun aBuildWithoutValuesDoesNotIncludeItAndNeverAsksForPlayServices() {
        var asked = false
        val googlePlay =
            googlePlayFor(
                null,
                playServices = {
                    asked = true
                    true
                },
                start = { error("Firebase started") },
            )
        assertEquals(GooglePlay.Unavailable.NotInBuild, googlePlay)
        assertFalse(asked)
    }

    @Test
    fun aPhoneWithoutPlayServicesDoesNotHaveIt() {
        val googlePlay =
            googlePlayFor(values, playServices = { false }, start = { error("Firebase started") })
        assertEquals(GooglePlay.Unavailable.NotOnPhone, googlePlay)
    }

    @Test
    fun firebaseStartsWithTheValuesOnlyOnceFcmIsAskedFor() {
        var started: FirebaseValues? = null
        val googlePlay =
            googlePlayFor(
                values,
                playServices = { true },
                start = {
                    started = it
                    pushed
                },
            )
        assertNull(started)
        assertSame(pushed, (googlePlay as GooglePlay.Available).push)
        assertEquals(values, started)
    }

    @Test
    fun whereItIsAvailableThePersonsChoiceStands() {
        val available = GooglePlay.Available { pushed }
        assertEquals(
            listOf(Delivery.GooglePlay, Delivery.Connection),
            listOf(Delivery.GooglePlay, Delivery.Connection).map(available::delivery),
        )
    }

    @Test
    fun whereItIsUnavailableThePhoneUsesTheConnection() {
        assertEquals(
            Delivery.Connection,
            GooglePlay.Unavailable.NotOnPhone.delivery(Delivery.GooglePlay),
        )
    }
}
