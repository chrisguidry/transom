package sh.transom.exchange

import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.font.FontFamily
import org.junit.Assert.assertEquals
import org.junit.Test

/** Markdown follows the web UI's rules, in `server/src/web/markdown.rs`, case by case. */
class MarkdownTest {
    private val markdown = Markdown(code = CODE) { "https://transom.example.com$it" }

    private fun blocks(text: String, vararg attachments: Attachment): List<Block> =
        markdown.body(text, attachments.toList()).blocks

    /** The text of [text], which must be one paragraph. */
    private fun paragraph(text: String): AnnotatedString =
        (blocks(text).single() as Block.Paragraph).text

    @Test
    fun rendersAParagraph() {
        assertEquals("The disk is at 94%.", paragraph("The disk is at **94%**.").text)
    }

    @Test
    fun rendersStrongEmphasisBold() {
        assertEquals(listOf("94%"), paragraph("The disk is at **94%**.").styled(Bold))
    }

    @Test
    fun rendersEmphasisItalic() {
        assertEquals(listOf("now"), paragraph("Do it *now*.").styled(Italic))
    }

    @Test
    fun rendersStrikethrough() {
        assertEquals(listOf("Friday"), paragraph("On ~~Friday~~ Monday.").styled(Struck))
    }

    @Test
    fun rendersCodeInTheCodeStyle() {
        assertEquals(listOf("df -h"), paragraph("Run `df -h` first.").styled(CODE))
    }

    @Test
    fun joinsSoftLineBreaksWithASpace() {
        assertEquals("one two", paragraph("one\ntwo").text)
    }

    @Test
    fun keepsHardLineBreaks() {
        assertEquals("one\ntwo", paragraph("one  \ntwo").text)
    }

    @Test
    fun showsRawHtmlBlocksAsText() {
        assertEquals(
            listOf(Block.Paragraph(AnnotatedString("<script>alert(1)</script>"))),
            blocks("<script>alert(1)</script>"),
        )
    }

    @Test
    fun showsInlineHtmlAsText() {
        assertEquals("and <b>bold</b>", paragraph("and <b>bold</b>").text)
    }

    @Test
    fun linksToTheWeb() {
        assertEquals(
            listOf("web" to "https://grafana/d", "plain" to "http://grafana/d"),
            paragraph("[web](https://grafana/d) [plain](http://grafana/d)").links(),
        )
    }

    @Test
    fun linksToMail() {
        assertEquals(listOf("mail" to "mailto:a@b"), paragraph("[mail](mailto:a@b)").links())
    }

    @Test
    fun linksToTransomsOwnPagesOnTheSessionsServer() {
        assertEquals(
            listOf("page" to "https://transom.example.com/exchanges/1"),
            paragraph("[page](/exchanges/1)").links(),
        )
    }

    @Test
    fun showsAnyOtherLinkAsItsText() {
        val text = paragraph("[bad](javascript:alert(1)) [far](//evil) [here](#top) [rel](a.png)")
        assertEquals(listOf("bad far here rel", "[]"), listOf(text.text, text.links().toString()))
    }

    @Test
    fun showsAnImageAsALinkToIt() {
        assertEquals(
            listOf("graph" to "https://tracker.example.com/pixel.png"),
            paragraph("![graph](https://tracker.example.com/pixel.png)").links(),
        )
    }

    @Test
    fun showsAnImageWithADisallowedAddressAsItsText() {
        val text = paragraph("![bad](javascript:x)")
        assertEquals(listOf("bad", "[]"), listOf(text.text, text.links().toString()))
    }

    @Test
    fun showsTheMessagesOwnImageWhereTheBodyPlacesIt() {
        assertEquals(
            listOf(
                Block.Paragraph(AnnotatedString("Before")),
                Block.Picture(GRAPH, "the graph"),
                Block.Paragraph(AnnotatedString("After")),
            ),
            blocks("Before\n\n![the graph](graph.png)\n\nAfter", GRAPH),
        )
    }

    @Test
    fun splitsAParagraphAroundAPlacedImage() {
        assertEquals(
            listOf(
                Block.Paragraph(AnnotatedString("See")),
                Block.Picture(GRAPH, "g"),
                Block.Paragraph(AnnotatedString("here")),
            ),
            blocks("See ![g](graph.png) here", GRAPH),
        )
    }

    @Test
    fun keepsLinksInEachPartOfASplitParagraph() {
        val split = blocks("[a](https://a) ![g](graph.png) [b](https://b)", GRAPH)
        assertEquals(
            listOf(listOf("a" to "https://a"), listOf("b" to "https://b")),
            split.filterIsInstance<Block.Paragraph>().map { it.text.links() },
        )
    }

    @Test
    fun namesTheImagesItPlaced() {
        val body = markdown.body("![the graph](graph.png)", listOf(GRAPH, OTHER))
        assertEquals(setOf("graph.png"), body.placed)
    }

    @Test
    fun showsAnImageOfItsOwnInsideOtherTextAsItsText() {
        val body = markdown.body("**![the graph](graph.png)**", listOf(GRAPH))
        assertEquals(
            listOf("the graph", emptySet<String>()),
            listOf(body.blocks.single().paragraphText(), body.placed),
        )
    }

    @Test
    fun rendersHeadings() {
        assertEquals(
            listOf(1 to "Deploy", 3 to "Details"),
            blocks("# Deploy\n\n### Details").map { (it as Block.Heading).level to it.text.text },
        )
    }

    @Test
    fun rendersCodeBlocks() {
        assertEquals(
            listOf(Block.Code("df -h\ndu -s"), Block.Code("indented")),
            blocks("```sh\ndf -h\ndu -s\n```\n\n    indented"),
        )
    }

    @Test
    fun rendersQuotes() {
        assertEquals(
            listOf(Block.Quote(listOf(Block.Paragraph(AnnotatedString("quoted"))))),
            blocks("> quoted"),
        )
    }

    @Test
    fun rendersThematicBreaks() {
        assertEquals(Block.Rule, blocks("one\n\n---\n\ntwo")[1])
    }

    @Test
    fun rendersBulletedLists() {
        val list = blocks("- one\n- two").single() as Block.Items
        assertEquals(listOf(null, "one", "two"), listOf(list.start) + list.items.map(::itemText))
    }

    @Test
    fun rendersNumberedListsFromTheirFirstNumber() {
        val list = blocks("3. three\n4. four").single() as Block.Items
        assertEquals(listOf(3, "three", "four"), listOf(list.start) + list.items.map(::itemText))
    }

    @Test
    fun rendersTaskLists() {
        val list = blocks("- [x] built\n- [ ] shipped\n- plain").single() as Block.Items
        assertEquals(
            listOf(true to "built", false to "shipped", null to "plain"),
            list.items.map { it.task to itemText(it) },
        )
    }

    @Test
    fun rendersTables() {
        assertEquals(
            Block.Table(
                header = listOf(AnnotatedString("Host"), AnnotatedString("Disk")),
                rows = listOf(listOf(AnnotatedString("nuc3"), AnnotatedString("91%"))),
            ),
            blocks("| Host | Disk |\n| --- | --- |\n| nuc3 | 91% |").single(),
        )
    }

    @Test
    fun rendersATableWithNoRows() {
        assertEquals(emptyList<Any>(), (blocks("| Host |\n| --- |").single() as Block.Table).rows)
    }

    @Test
    fun dropsLinkReferenceDefinitions() {
        assertEquals(
            listOf("see" to "https://x.example"),
            paragraph("[see][x]\n\n[x]: https://x.example").links(),
        )
    }
}

private val CODE = SpanStyle(fontFamily = FontFamily.Monospace)

private val GRAPH = Attachment("graph.png", "image/png", 10, "/exchanges/1/attachments/graph.png")
private val OTHER = Attachment("other.png", "image/png", 10, "/exchanges/1/attachments/other.png")

/** The parts of the text that have exactly [style]. */
private fun AnnotatedString.styled(style: SpanStyle): List<String> =
    spanStyles.filter { it.item == style }.map { text.substring(it.start, it.end) }

/** Each link's text and where it goes. */
private fun AnnotatedString.links(): List<Pair<String, String>> =
    getLinkAnnotations(0, length).map {
        text.substring(it.start, it.end) to (it.item as LinkAnnotation.Url).url
    }

private fun Block.paragraphText(): String = (this as Block.Paragraph).text.text

private fun itemText(item: Item): String = item.blocks.single().paragraphText()
