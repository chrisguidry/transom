package sh.transom.wake

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test

/**
 * FCM wakes the phone only in a build with all of the Firebase project's values, on a phone with
 * Google Play services.
 */
class PushForTest {
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
    fun aBuildWithoutValuesNeitherAsksForPlayServicesNorStartsFirebase() {
        var asked = false
        val push =
            pushFor(
                null,
                playServices = {
                    asked = true
                    true
                },
                start = { error("Firebase started") },
            )
        assertNull(push)
        assertFalse(asked)
    }

    @Test
    fun aPhoneWithoutPlayServicesDoesNotStartFirebase() {
        assertNull(pushFor(values, playServices = { false }, start = { error("Firebase started") }))
    }

    @Test
    fun aBuildWithValuesOnAPhoneWithPlayServicesStartsFirebase() {
        var started: FirebaseValues? = null
        val push =
            pushFor(
                values,
                playServices = { true },
                start = {
                    started = it
                    pushed
                },
            )
        assertSame(pushed, push)
        assertEquals(values, started)
    }
}
