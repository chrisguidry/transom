package sh.transom.notifications

import android.graphics.Typeface
import android.text.SpannableStringBuilder
import android.text.Spanned
import android.text.style.StrikethroughSpan
import android.text.style.StyleSpan
import android.text.style.TypefaceSpan
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import sh.transom.exchange.Block
import sh.transom.exchange.Markdown
import sh.transom.exchange.Message

/**
 * A message as a notification's text: its summary as a bold first line, then its body, which
 * [markdown] renders with the web UI's rules, as styled plain text. A notification draws no
 * Markdown, so each block becomes lines of text with the bold, italics, strikethrough, and code of
 * its spans, and the message's own images, which a notification does not draw, are left out.
 */
internal fun notificationText(message: Message, markdown: Markdown): CharSequence {
    val text = SpannableStringBuilder()
    message.summary?.takeIf { it.isNotEmpty() }?.let { text.styled(it, StyleSpan(Typeface.BOLD)) }
    val body = markdown.body(message.body.orEmpty(), message.attachments).blocks
    text.blocks(body, "")
    return text
}

/** Markdown for notifications: code in a monospace font, and links drawn as their text. */
internal fun notificationMarkdown(resolve: (String) -> String) =
    Markdown(code = SpanStyle(fontFamily = FontFamily.Monospace), resolve = resolve)

/** Appends each of [blocks] on lines of its own, each line after [indent]. */
private fun SpannableStringBuilder.blocks(blocks: List<Block>, indent: String) {
    // A when with no subject: a when over the sealed block compiles a branch for a kind of block
    // that cannot exist, which no test can reach. A rule and a picture add no text.
    for (block in blocks) {
        when {
            block is Block.Paragraph -> line(indent, block.text)
            block is Block.Heading -> line(indent, block.text, StyleSpan(Typeface.BOLD))
            block is Block.Code -> line(indent, AnnotatedString(block.text), monospace())
            block is Block.Quote -> blocks(block.blocks, "$indent  ")
            block is Block.Items ->
                block.items.forEachIndexed { index, item ->
                    val marker = block.start?.let { "${it + index}." } ?: "•"
                    val task = item.task?.let { if (it) " ☑" else " ☐" }.orEmpty()
                    blocks(item.blocks, "$indent$marker$task ")
                }
            block is Block.Table -> {
                line(indent, joined(block.header), StyleSpan(Typeface.BOLD))
                block.rows.forEach { line(indent, joined(it)) }
            }
        }
    }
}

/**
 * Starts a line after what came before, with [indent], then [text] in its own styles and [span].
 */
private fun SpannableStringBuilder.line(indent: String, text: AnnotatedString, span: Any? = null) {
    if (isNotEmpty()) append('\n')
    append(indent)
    val start = length
    annotated(text)
    span?.let { setSpan(it, start, length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE) }
}

private fun SpannableStringBuilder.styled(text: String, span: Any) {
    val start = length
    append(text)
    setSpan(span, start, length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
}

/** Appends [text] with a span for each of its styles that a notification can draw. */
private fun SpannableStringBuilder.annotated(text: AnnotatedString) {
    val start = length
    append(text.text)
    for (range in text.spanStyles) {
        for (span in spans(range.item)) {
            setSpan(span, start + range.start, start + range.end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        }
    }
}

private fun spans(style: SpanStyle): List<Any> = buildList {
    if ((style.fontWeight ?: FontWeight.Normal) >= FontWeight.SemiBold)
        add(StyleSpan(Typeface.BOLD))
    if (style.fontStyle == FontStyle.Italic) add(StyleSpan(Typeface.ITALIC))
    if (style.textDecoration?.contains(TextDecoration.LineThrough) == true) add(StrikethroughSpan())
    if (style.fontFamily == FontFamily.Monospace) add(monospace())
}

private fun joined(cells: List<AnnotatedString>): AnnotatedString =
    AnnotatedString.Builder()
        .apply {
            cells.forEachIndexed { index, cell ->
                if (index > 0) append(" · ")
                append(cell)
            }
        }
        .toAnnotatedString()

private fun monospace() = TypefaceSpan("monospace")
