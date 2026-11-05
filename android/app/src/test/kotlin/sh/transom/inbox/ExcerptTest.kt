package sh.transom.inbox

import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized
import sh.transom.exchange.Message

/** The line under a row's title, as the web's `excerpt` makes it. */
@RunWith(Parameterized::class)
class ExcerptTest(private val message: Message, private val excerpt: String) {
    @Test fun isTheBodysFirstLineUnderASummary() = assertEquals(excerpt, message.excerpt)

    companion object {
        @JvmStatic
        @Parameterized.Parameters(name = "{1}")
        fun cases() =
            listOf(
                arrayOf<Any>(Message(body = "The title already"), ""),
                arrayOf<Any>(Message(summary = "Down", body = "\n\nFirst\nSecond"), "First"),
                arrayOf<Any>(Message(summary = "Down"), ""),
                arrayOf<Any>(
                    Message(summary = "Down", body = "é".repeat(121)),
                    "é".repeat(120) + "…",
                ),
            )
    }
}
