package sh.transom.notifications

import android.graphics.Typeface
import android.text.Spanned
import android.text.style.StrikethroughSpan
import android.text.style.StyleSpan
import android.text.style.TypefaceSpan
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.exchange.Attachment
import sh.transom.exchange.Message

/** A message as a notification's text: the summary in bold, then the body as styled lines. */
@RunWith(RobolectricTestRunner::class)
class NotificationTextTest {
    private val markdown = notificationMarkdown { "https://transom.example.com$it" }

    private fun text(
        summary: String? = null,
        body: String? = null,
        attached: List<Attachment> = emptyList(),
    ) = notificationText(Message(summary, body, attachments = attached), markdown) as Spanned

    /** The parts of [text] that [kind] spans cover, each as the text it covers. */
    private fun <T : Any> Spanned.covered(kind: Class<T>, which: (T) -> Boolean = { true }) =
        getSpans(0, length, kind).filter(which).map {
            subSequence(getSpanStart(it), getSpanEnd(it)).toString()
        }

    private fun Spanned.bold() = covered(StyleSpan::class.java) { it.style == Typeface.BOLD }

    @Test
    fun theSummaryIsABoldFirstLine() {
        val text = text("Disk almost full", "The disk is at 91%.")
        assertEquals("Disk almost full\nThe disk is at 91%.", text.toString())
        assertEquals(listOf("Disk almost full"), text.bold())
    }

    @Test
    fun aBodyAloneHasNoSummaryLine() =
        assertEquals("Just the body", text(body = "Just the body").toString())

    @Test
    fun inlineStylesBecomeSpans() {
        val text = text(body = "**bold** *italic* ~~gone~~ `code`")
        assertEquals("bold italic gone code", text.toString())
        assertEquals(listOf("bold"), text.bold())
        assertEquals(
            listOf("italic"),
            text.covered(StyleSpan::class.java) { it.style == Typeface.ITALIC },
        )
        assertEquals(listOf("gone"), text.covered(StrikethroughSpan::class.java))
        assertEquals(listOf("code"), text.covered(TypefaceSpan::class.java))
    }

    @Test
    fun aLinkIsItsText() =
        assertEquals("see the graph", text(body = "see [the graph](/d/disk)").toString())

    @Test
    fun headingsAndCodeBlocksAreLinesOfTheirOwn() {
        val text = text(body = "# Disk\n\n```\ndf -h\n```")
        assertEquals("Disk\ndf -h", text.toString())
        assertEquals(listOf("Disk"), text.bold())
        assertEquals(listOf("df -h"), text.covered(TypefaceSpan::class.java))
    }

    @Test
    fun listsKeepTheirMarkers() =
        assertEquals(
            "• one\n• two\n3. three\n4. four\n• ☑ done\n• ☐ to do",
            text(body = "- one\n- two\n\n3. three\n4. four\n\n- [x] done\n- [ ] to do").toString(),
        )

    @Test fun aQuoteIsIndented() = assertEquals("  quoted", text(body = "> quoted").toString())

    @Test
    fun aTableIsARowPerLine() {
        val text = text(body = "| Host | Disk |\n| --- | --- |\n| db1 | 91% |")
        assertEquals("Host · Disk\ndb1 · 91%", text.toString())
        assertEquals(listOf("Host · Disk"), text.bold())
    }

    @Test
    fun aRuleAndAnAttachedImageAddNoText() {
        val graph = Attachment("graph.png", "image/png", 10, "/exchanges/x/attachments/graph.png")
        assertEquals(
            "above\nbelow",
            text(body = "above\n\n---\n\n![graph](graph.png)\n\nbelow", attached = listOf(graph))
                .toString(),
        )
    }
}
