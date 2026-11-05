package sh.transom.exchange

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.toUpperCase
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import sh.transom.theme.BodyLineHeight

/**
 * The [Markdown] renderer in the theme's colors, as `transom.css` styles a body: links in the
 * accent and underlined, and code on the soft color. [resolve] makes a path absolute on the
 * session's server.
 */
@Composable
fun rememberMarkdown(resolve: (String) -> String): Markdown {
    val colors = MaterialTheme.colorScheme
    return remember(colors, resolve) {
        Markdown(
            code =
                SpanStyle(
                    fontFamily = FontFamily.Monospace,
                    fontSize = 0.9.em,
                    background = colors.surfaceVariant,
                ),
            links =
                TextLinkStyles(
                    SpanStyle(color = colors.primary, textDecoration = TextDecoration.Underline)
                ),
            resolve = resolve,
        )
    }
}

/**
 * [blocks], [spacing] apart, as a browser lays out the HTML the web UI renders. [picture] draws
 * each of the message's own images where the body placed it.
 */
@Composable
fun MarkdownBlocks(
    blocks: List<Block>,
    picture: @Composable (Block.Picture) -> Unit,
    modifier: Modifier = Modifier,
    spacing: Dp = 16.dp,
) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(spacing)) {
        blocks.forEach { Block(it, picture) }
    }
}

@Composable
private fun Block(block: Block, picture: @Composable (Block.Picture) -> Unit) {
    // A when with no subject: a when over the sealed block compiles a branch for a kind of block
    // that cannot exist, which no test can reach.
    when {
        block is Block.Paragraph -> Text(block.text, lineHeight = BodyLineHeight)
        block is Block.Heading -> Heading(block)
        block is Block.Code -> Code(block.text)
        // A browser indents a quote by 40 px and draws nothing beside it.
        block is Block.Quote ->
            MarkdownBlocks(block.blocks, picture, Modifier.padding(start = 40.dp))
        block is Block.Items -> Items(block, picture)
        block is Block.Table -> Table(block)
        block is Block.Rule ->
            HorizontalDivider(thickness = 1.dp, color = MaterialTheme.colorScheme.outline)
        block is Block.Picture -> picture(block)
    }
}

/**
 * A heading at the sizes a browser gives each level, except the second, which `transom.css` draws
 * as a section's label everywhere on the page, a message's body included.
 */
@Composable
private fun Heading(heading: Block.Heading) {
    val second = heading.level == 2
    Text(
        text = if (second) heading.text.toUpperCase() else heading.text,
        modifier = Modifier.semantics { heading() },
        color =
            if (second) MaterialTheme.colorScheme.onSurfaceVariant
            else MaterialTheme.colorScheme.onSurface,
        fontSize = HEADING_SIZES[heading.level - 1].sp,
        fontWeight = if (heading.level <= 2) FontWeight.SemiBold else FontWeight.Bold,
        letterSpacing = if (second) 0.04.em else 0.em,
        lineHeight = 1.3.em,
    )
}

/** Each heading level's size: `transom.css` sets the first two, and a browser the rest. */
private val HEADING_SIZES = listOf(24f, 13.6f, 18.72f, 16f, 13.28f, 10.72f)

/** A code block, as `.body pre`: unwrapped, on the soft color, scrolling sideways when wide. */
@Composable
private fun Code(text: String) {
    Text(
        text = text,
        modifier =
            Modifier.fillMaxWidth()
                .clip(RoundedCornerShape(4.dp))
                .background(MaterialTheme.colorScheme.surfaceVariant)
                .horizontalScroll(rememberScrollState())
                .padding(12.dp),
        fontFamily = FontFamily.Monospace,
        fontSize = 14.4.sp,
        lineHeight = BodyLineHeight,
        softWrap = false,
    )
}

/** A list, with each item's bullet or number, and its checkbox when it is a task. */
@Composable
private fun Items(list: Block.Items, picture: @Composable (Block.Picture) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        list.items.forEachIndexed { index, item ->
            Row {
                Text(
                    list.start?.let { "${it + index}." } ?: "•",
                    Modifier.width(30.dp).padding(end = 8.dp),
                    textAlign = TextAlign.End,
                    lineHeight = BodyLineHeight,
                )
                item.task?.let {
                    Text(
                        if (it) "☑" else "☐",
                        Modifier.padding(end = 6.dp),
                        lineHeight = BodyLineHeight,
                    )
                }
                MarkdownBlocks(item.blocks, picture, Modifier.weight(1f), spacing = 8.dp)
            }
        }
    }
}

/**
 * A table, one column at a time, each as wide as its widest cell, with a rule under each row. A
 * cell stays on one line, so the rows line up across the columns, and a wide table scrolls
 * sideways.
 */
@Composable
private fun Table(table: Block.Table) {
    Row(Modifier.horizontalScroll(rememberScrollState())) {
        table.header.forEachIndexed { column, heading ->
            Column(Modifier.width(IntrinsicSize.Max)) {
                Cell(heading, FontWeight.SemiBold)
                table.rows.forEach { Cell(it.getOrElse(column) { AnnotatedString("") }) }
            }
        }
    }
}

@Composable
private fun Cell(text: AnnotatedString, weight: FontWeight? = null) {
    Column(Modifier.fillMaxWidth()) {
        Text(
            text,
            Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
            fontWeight = weight,
            lineHeight = BodyLineHeight,
            softWrap = false,
        )
        HorizontalDivider(thickness = 1.dp, color = MaterialTheme.colorScheme.outline)
    }
}
