package sh.transom.sync

import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized

@RunWith(Parameterized::class)
class RefusedTest(private val status: Int, private val transient: Boolean) {
    @Test
    fun aFailureForNowIsWorthSendingAgain() {
        assertEquals(transient, Refused(status, "").transient)
    }

    companion object {
        @JvmStatic
        @Parameterized.Parameters(name = "{0}")
        fun cases() =
            listOf(
                arrayOf<Any>(400, false),
                arrayOf<Any>(401, true),
                arrayOf<Any>(403, false),
                arrayOf<Any>(404, false),
                arrayOf<Any>(408, true),
                arrayOf<Any>(409, false),
                arrayOf<Any>(410, false),
                arrayOf<Any>(429, true),
                arrayOf<Any>(500, true),
                arrayOf<Any>(503, true),
            )
    }
}

class RefusedBodyTest {
    @Test
    fun aFailureThatIsNotTransomsKeepsItsWholeBody() {
        assertEquals("<h1>Bad gateway</h1>", Refused.of(502, "<h1>Bad gateway</h1>").reason)
    }
}
