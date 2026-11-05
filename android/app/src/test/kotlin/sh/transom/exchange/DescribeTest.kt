package sh.transom.exchange

import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Test

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
    fun aTimeIsShownInUtcToTheMinute() {
        assertEquals("2026-11-03 21:14 UTC", Instant.parse("2026-11-03T21:14:59.9Z").shown())
    }
}
