package sh.transom.exchange

import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import org.commonmark.ext.gfm.strikethrough.Strikethrough
import org.commonmark.ext.gfm.strikethrough.StrikethroughExtension
import org.commonmark.ext.gfm.tables.TableBlock
import org.commonmark.ext.gfm.tables.TableHead
import org.commonmark.ext.gfm.tables.TablesExtension
import org.commonmark.ext.task.list.items.TaskListItemMarker
import org.commonmark.ext.task.list.items.TaskListItemsExtension
import org.commonmark.node.BlockQuote
import org.commonmark.node.BulletList
import org.commonmark.node.Code
import org.commonmark.node.Emphasis
import org.commonmark.node.FencedCodeBlock
import org.commonmark.node.HardLineBreak
import org.commonmark.node.Heading
import org.commonmark.node.HtmlBlock
import org.commonmark.node.HtmlInline
import org.commonmark.node.Image
import org.commonmark.node.IndentedCodeBlock
import org.commonmark.node.Link
import org.commonmark.node.ListBlock
import org.commonmark.node.Node
import org.commonmark.node.OrderedList
import org.commonmark.node.Paragraph
import org.commonmark.node.SoftLineBreak
import org.commonmark.node.StrongEmphasis
import org.commonmark.node.Text
import org.commonmark.node.ThematicBreak
import org.commonmark.parser.Parser

/** A message's body, rendered: its [blocks], and the names of the attachments they [placed]. */
data class Body(val blocks: List<Block>, val placed: Set<String>)

/** One block of a rendered body, drawn by `MarkdownBlocks`. */
sealed interface Block {
    data class Paragraph(val text: AnnotatedString) : Block

    data class Heading(val level: Int, val text: AnnotatedString) : Block

    data class Code(val text: String) : Block

    data class Quote(val blocks: List<Block>) : Block

    /** A list, numbered from [start], or bulleted when [start] is `null`. */
    data class Items(val start: Int?, val items: List<Item>) : Block

    data class Table(val header: List<AnnotatedString>, val rows: List<List<AnnotatedString>>) :
        Block

    data object Rule : Block

    /** One of the message's own images, where the body placed it, described by [alt]. */
    data class Picture(val attachment: Attachment, val alt: String) : Block
}

/** A list's item, with a checkbox that is [task] checked, or none when it is `null`. */
data class Item(val task: Boolean?, val blocks: List<Block>)

/**
 * Markdown from senders, rendered with the web UI's rules: tables, strikethrough, and task lists;
 * raw HTML shown as text; links only to the web, mail, and transom's own pages, which [resolve]
 * makes absolute on the session's server; and images shown as links rather than loaded, so a
 * message cannot learn when it is read by loading one. The message's own attachments are the
 * exception: transom serves them, so loading one tells no one else anything.
 */
class Markdown(
    private val code: SpanStyle,
    private val links: TextLinkStyles? = null,
    private val resolve: (String) -> String,
) {
    fun body(markdown: String, attachments: List<Attachment> = emptyList()): Body {
        val reading = Reading(attachments)
        return Body(reading.blocks(parser.parse(markdown)), reading.placed)
    }

    private inner class Reading(private val attachments: List<Attachment>) {
        val placed = linkedSetOf<String>()

        fun blocks(parent: Node): List<Block> = children(parent).flatMap(::block)

        /**
         * What [node] renders as. A node this app does not draw, such as a link reference
         * definition, renders as nothing.
         */
        private fun block(node: Node): List<Block> =
            when (node) {
                is Paragraph -> paragraph(node)
                is Heading -> listOf(Block.Heading(node.level, inline(node)))
                is BlockQuote -> listOf(Block.Quote(blocks(node)))
                is BulletList -> listOf(Block.Items(null, items(node)))
                is OrderedList -> listOf(Block.Items(node.markerStartNumber, items(node)))
                is FencedCodeBlock -> listOf(Block.Code(node.literal.trimEnd('\n')))
                is IndentedCodeBlock -> listOf(Block.Code(node.literal.trimEnd('\n')))
                is HtmlBlock -> listOf(Block.Paragraph(AnnotatedString(node.literal.trimEnd())))
                is ThematicBreak -> listOf(Block.Rule)
                is TableBlock -> listOf(table(node))
                else -> emptyList()
            }

        /**
         * A paragraph, split around each of the message's own images it places, which are drawn as
         * blocks between its parts. Only an image directly in the paragraph is placed; one inside a
         * link or emphasis shows as its text, and with the rest after the body.
         */
        private fun paragraph(node: Paragraph): List<Block> {
            val parts = mutableListOf<Block>()
            var text = AnnotatedString.Builder()
            fun flush() {
                val part = text.toAnnotatedString().trimmed()
                if (part.isNotEmpty()) parts += Block.Paragraph(part)
                text = AnnotatedString.Builder()
            }
            for (child in children(node)) {
                val attachment = (child as? Image)?.let(::attached)
                if (attachment == null) {
                    text.inline(child)
                } else {
                    flush()
                    placed += attachment.name
                    parts += Block.Picture(attachment, inline(child).text)
                }
            }
            flush()
            return parts
        }

        private fun items(list: ListBlock): List<Item> =
            children(list).map { item ->
                Item((item.firstChild as? TaskListItemMarker)?.isChecked, blocks(item))
            }

        private fun table(node: TableBlock): Block.Table {
            val (head, body) = children(node).partition { it is TableHead }
            fun rows(sections: List<Node>) =
                sections.flatMap(::children).map { row -> children(row).map(::inline) }
            return Block.Table(rows(head).single(), rows(body))
        }

        private fun inline(parent: Node): AnnotatedString =
            AnnotatedString.Builder()
                .apply { children(parent).forEach { inline(it) } }
                .toAnnotatedString()

        /**
         * Appends [node], one of the inline nodes the parser and its extensions make, which are all
         * named here. A task list's checkbox is a node of the list item, not of its text.
         */
        private fun AnnotatedString.Builder.inline(node: Node) {
            when (node) {
                is Text -> append(node.literal)
                is Code -> withStyle(code) { append(node.literal) }
                is HtmlInline -> append(node.literal)
                is SoftLineBreak -> append(' ')
                is HardLineBreak -> append('\n')
                is Emphasis -> withStyle(Italic) { inlines(node) }
                is StrongEmphasis -> withStyle(Bold) { inlines(node) }
                is Strikethrough -> withStyle(Struck) { inlines(node) }
                is Image ->
                    if (attached(node) != null) inlines(node) else link(node.destination, node)
                is Link -> link(node.destination, node)
            }
        }

        private fun AnnotatedString.Builder.inlines(parent: Node) =
            children(parent).forEach { inline(it) }

        /** [node]'s text, linked to [destination] when a message may link there. */
        private fun AnnotatedString.Builder.link(destination: String, node: Node) {
            val url = linkable(destination)
            if (url == null) inlines(node)
            else withLink(LinkAnnotation.Url(url, links)) { inlines(node) }
        }

        private fun attached(image: Image): Attachment? = attachments.firstOrNull {
            it.name == image.destination
        }

        /**
         * Where a link to [url] may go: the web, mail, or a page of transom's own, made absolute.
         * Anything else, such as `javascript:`, a page of another host named by `//`, or a fragment
         * of a page the app does not have, is no link.
         */
        private fun linkable(url: String): String? =
            when {
                url.startsWith("https://") ||
                    url.startsWith("http://") ||
                    url.startsWith("mailto:") -> url
                url.startsWith("/") && !url.startsWith("//") -> resolve(url)
                else -> null
            }
    }
}

internal val Bold = SpanStyle(fontWeight = FontWeight.Bold)
internal val Italic = SpanStyle(fontStyle = FontStyle.Italic)
internal val Struck = SpanStyle(textDecoration = TextDecoration.LineThrough)

private val parser: Parser =
    Parser.builder()
        .extensions(
            listOf(
                TablesExtension.create(),
                StrikethroughExtension.create(),
                TaskListItemsExtension.create(),
            )
        )
        .build()

private fun children(parent: Node): List<Node> =
    generateSequence(parent.firstChild) { it.next }.toList()

/** The text without the white space at either end, keeping its styles and links. */
private fun AnnotatedString.trimmed(): AnnotatedString {
    val start = text.indexOfFirst { !it.isWhitespace() }
    if (start < 0) return AnnotatedString("")
    return subSequence(start, text.indexOfLast { !it.isWhitespace() } + 1)
}
