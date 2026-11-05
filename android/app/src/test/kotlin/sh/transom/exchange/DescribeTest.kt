package sh.transom.exchange

import java.time.Instant
import java.time.ZoneOffset
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized

/** A message's title and a time, said as the web UI's `pages.rs` says them. */
class DescribeTest {
    @Test
    fun aMessageIsCalledByItsSummary() {
        assertEquals("Deploy?", Message(summary = "Deploy?", body = "The body").title)
    }

    @Test
    fun aMessageWithNoSummaryIsCalledByItsFirstLine() {
        assertEquals("Disk is full", Message(summary = "", body = "Disk is full\nOn nuc3").title)
    }

    @Test
    fun aLongFirstLineIsCut() {
        assertEquals("a".repeat(80) + "…", Message(body = "a".repeat(81)).title)
    }

    @Test
    fun aMessageWithNothingIsCalledNothing() {
        assertEquals("", Message().title)
    }

    @Test
    fun aTitleFromTheBodyLosesItsMarkdown() {
        assertEquals("Backup done", Message(body = "**Backup** done").title)
    }

    @Test
    fun aPageSaysItsTimeInFull() {
        val at = Instant.parse("2026-11-05T17:29:00Z")
        assertEquals(
            listOf("Thu, Nov 5 at 5:29 PM", "Thu, Nov 5 at 17:29"),
            listOf(at.inFull(false, ZoneOffset.UTC), at.inFull(true, ZoneOffset.UTC)),
        )
    }
}

/** How long ago a row says something arrived, by the phone's own days. */
@RunWith(Parameterized::class)
class AgoTest(private val at: String, private val said: String) {
    @Test
    fun saysHowLongAgo() {
        val now = Instant.parse("2026-11-05T12:00:00Z")
        assertEquals(said, Instant.parse(at).ago(now, ZoneOffset.UTC))
    }

    companion object {
        @JvmStatic
        @Parameterized.Parameters(name = "{1}")
        fun cases() =
            listOf(
                arrayOf<Any>("2026-11-05T11:59:30Z", "now"),
                arrayOf<Any>("2026-11-05T11:56:00Z", "4m"),
                arrayOf<Any>("2026-11-05T09:00:00Z", "3h"),
                arrayOf<Any>("2026-11-04T20:00:00Z", "Yesterday"),
                arrayOf<Any>("2026-11-03T08:00:00Z", "Tue"),
                arrayOf<Any>("2026-10-02T08:00:00Z", "Oct 2"),
                arrayOf<Any>("2025-11-02T08:00:00Z", "Nov 2, 2025"),
            )
    }
}

/** The excerpt's first line, without its Markdown, as the web's `markdown::first_line`. */
@RunWith(Parameterized::class)
class FirstLineTest(private val markdown: String, private val line: String) {
    @Test fun losesItsMarks() = assertEquals(line, firstLine(markdown))

    companion object {
        @JvmStatic
        @Parameterized.Parameters(name = "{1}")
        fun cases() =
            listOf(
                arrayOf<Any>(
                    "A pod in **paperless/web** restarted.",
                    "A pod in paperless/web restarted.",
                ),
                arrayOf<Any>("Took 4 minutes.\nNothing failed.", "Took 4 minutes."),
                arrayOf<Any>("\n\n- first item\n- second", "first item"),
                arrayOf<Any>("# Heading\n\nBody", "Heading"),
                arrayOf<Any>("See [the run](https://ci/7) and `logs`.", "See the run and logs."),
                arrayOf<Any>("<b>raw</b>", "<b>raw</b>"),
                arrayOf<Any>("one  \ntwo", "one"),
                arrayOf<Any>("", ""),
            )
    }
}
