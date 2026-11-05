package sh.transom.exchange

import org.commonmark.ext.gfm.strikethrough.StrikethroughExtension
import org.commonmark.node.Code
import org.commonmark.node.HardLineBreak
import org.commonmark.node.Heading
import org.commonmark.node.HtmlBlock
import org.commonmark.node.HtmlInline
import org.commonmark.node.Node
import org.commonmark.node.Paragraph
import org.commonmark.node.SoftLineBreak
import org.commonmark.node.Text
import org.commonmark.parser.Parser

/**
 * The first line of [markdown]'s text, with its marks taken out, as a list shows a message under
 * its title: `**94%** full` reads `94% full`, a list item loses its bullet, and a link keeps its
 * words. The web's `markdown::first_line` makes the same line.
 */
internal fun firstLine(markdown: String): String {
    val line = StringBuilder()
    walk(parser.parse(markdown), line)
    return line.toString()
}

/** Appends [node]'s text to [line], and returns `true` once the line is whole. */
private fun walk(node: Node, line: StringBuilder): Boolean {
    when (node) {
        is Text -> line.append(node.literal)
        is Code -> line.append(node.literal)
        is HtmlInline -> line.append(node.literal)
        is HtmlBlock -> line.append(node.literal.trim())
        is SoftLineBreak,
        is HardLineBreak -> if (line.isNotBlank()) return true
    }
    for (child in generateSequence(node.firstChild) { it.next }) {
        if (walk(child, line)) return true
    }
    return (node is Paragraph || node is Heading) && line.isNotBlank()
}

private val parser: Parser =
    Parser.builder().extensions(listOf(StrikethroughExtension.create())).build()
