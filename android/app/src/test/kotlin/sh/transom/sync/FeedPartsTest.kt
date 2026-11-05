package sh.transom.sync

import kotlin.time.Duration.Companion.seconds
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized

@RunWith(Parameterized::class)
class BackoffTest(private val failures: Int, private val seconds: Long) {
    @Test
    fun doublesUpToAMinute() {
        assertEquals(seconds.seconds, Backoff().after(failures))
    }

    companion object {
        @JvmStatic
        @Parameterized.Parameters(name = "{0}")
        fun cases() =
            listOf(
                arrayOf<Any>(0, 1L),
                arrayOf<Any>(1, 2L),
                arrayOf<Any>(5, 32L),
                arrayOf<Any>(6, 60L),
                arrayOf<Any>(1000, 60L),
            )
    }
}

@RunWith(Parameterized::class)
class UnknownEventTest(private val type: String) {
    @Test
    fun skipsEventsThatAreNotChanges() {
        assertNull(decode(type, "transom's storage is unavailable"))
    }

    companion object {
        @JvmStatic @Parameterized.Parameters(name = "{0}") fun cases() = listOf("error", "")
    }
}
