package sh.transom.pictures

import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized

/** The letters that stand in for a missing picture, as the web's `initials` makes them. */
@RunWith(Parameterized::class)
class InitialsTest(private val name: String, private val initials: String) {
    @Test fun areTheFirstLettersOfTheFirstTwoWords() = assertEquals(initials, initials(name))

    companion object {
        @JvmStatic
        @Parameterized.Parameters(name = "{0}")
        fun cases() =
            listOf(
                arrayOf<Any>("Alice Liddell", "AL"),
                arrayOf<Any>("@review-bot", "RB"),
                arrayOf<Any>("@chris.guidry.extra", "CG"),
                arrayOf<Any>("alertmanager", "A"),
                arrayOf<Any>("@", "?"),
            )
    }
}
