package sh.transom.exchange

import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized

@RunWith(Parameterized::class)
class ChoiceKeyTest(private val choice: Choice, private val key: String) {
    @Test
    fun aReplyNamesAChoiceByItsIdOrItsLabel() {
        assertEquals(key, choice.key)
    }

    companion object {
        @JvmStatic
        @Parameterized.Parameters(name = "{0}")
        fun cases() =
            listOf(
                arrayOf<Any>(Choice("Restart", id = "restart"), "restart"),
                arrayOf<Any>(Choice("Leave it"), "Leave it"),
            )
    }
}

@RunWith(Parameterized::class)
class StateTest(private val state: State, private val final: Boolean) {
    @Test
    fun onlyPendingAndOpenCanStillChange() {
        assertEquals(final, state.isFinal)
    }

    companion object {
        @JvmStatic
        @Parameterized.Parameters(name = "{0}")
        fun cases() =
            listOf(
                arrayOf<Any>(State.PENDING, false),
                arrayOf<Any>(State.OPEN, false),
                arrayOf<Any>(State.REPLIED, true),
                arrayOf<Any>(State.EXPIRED, true),
                arrayOf<Any>(State.WITHDRAWN, true),
                arrayOf<Any>(State.DECLINED, true),
            )
    }
}
