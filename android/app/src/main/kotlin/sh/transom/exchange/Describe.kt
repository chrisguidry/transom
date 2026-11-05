package sh.transom.exchange

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withLink
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import sh.transom.sync.InboxEntry
import sh.transom.theme.MetaText
import sh.transom.theme.appendState

/**
 * What a message is called in a list and over its own page, as the web UI calls it: its summary, or
 * the first line of its body, cut at 80 characters.
 */
val Message.title: String
    get() {
        if (!summary.isNullOrEmpty()) return summary
        val line = body.orEmpty().lineSequence().first()
        val characters = line.codePoints().toArray()
        return if (characters.size <= TITLE_LENGTH) line
        else String(characters, 0, TITLE_LENGTH) + "…"
    }

private const val TITLE_LENGTH = 80

/** A time as the web UI writes it, such as `2026-11-03 21:14 UTC`. */
fun Instant.shown(): String = TIME.format(this)

private val TIME = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm 'UTC'").withZone(ZoneOffset.UTC)

/**
 * Where [this] entry stands, as the meta line says it: the exchange's state, such as `open`, or
 * `sending` while a reply this phone queued is on its way.
 */
val InboxEntry.stateName: String
    get() = if (sending != null) "sending" else exchange.state.name.lowercase()

/**
 * `from → to · when · state`, as the web UI's inbox card and exchange page both begin, each party
 * by its display name, or its handle when it has none. With [handles], as on the exchange's page, a
 * display name has its handle beside it. With [onGroup], the recipient is a group, and its name is
 * a link that calls it.
 */
@Composable
fun ExchangeMeta(
    entry: InboxEntry,
    onGroup: (() -> Unit)?,
    modifier: Modifier = Modifier,
    handles: Boolean = false,
) {
    MetaText(metaLine(entry, onGroup, handles), modifier)
}

/** The meta line's text, which a read row of the inbox draws after the title, as [ExchangeMeta]. */
@Composable
internal fun metaLine(entry: InboxEntry, onGroup: (() -> Unit)?, handles: Boolean) =
    buildAnnotatedString {
        val exchange = entry.exchange
        val link = TextLinkStyles(SpanStyle(MaterialTheme.colorScheme.primary))
        val party = { handle: String, name: String? ->
            if (name == null) handle else if (handles) "$name $handle" else name
        }
        append("${party(exchange.from, exchange.fromName)} → ")
        val to = party(exchange.to, exchange.toName)
        if (onGroup == null) append(to)
        else withLink(LinkAnnotation.Clickable(exchange.to, link) { onGroup() }) { append(to) }
        append(" · ${exchange.created.shown()} · ")
        appendState(entry.stateName)
    }
