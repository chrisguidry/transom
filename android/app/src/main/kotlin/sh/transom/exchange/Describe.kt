package sh.transom.exchange

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withLink
import java.time.Duration
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.temporal.ChronoUnit
import java.util.Locale
import sh.transom.sync.InboxEntry
import sh.transom.theme.MetaText

/**
 * What a message is called in a list and over its own page, as the web UI calls it: its summary, or
 * the first line of its body without its Markdown, cut at 80 characters.
 */
val Message.title: String
    get() {
        if (!summary.isNullOrEmpty()) return summary
        return cut(firstLine(body.orEmpty()), TITLE_LENGTH)
    }

/**
 * The first line of the body, without its Markdown, to show under the summary in a list, cut at 120
 * characters, or nothing when the body's first line is already the title, as the web makes it.
 */
val Message.excerpt: String
    get() = if (summary.isNullOrEmpty()) "" else cut(firstLine(body.orEmpty()).trim(), 120)

private fun cut(line: String, most: Int): String {
    val characters = line.codePoints().toArray()
    return if (characters.size <= most) line else String(characters, 0, most) + "…"
}

private const val TITLE_LENGTH = 80

/**
 * How long ago [this] was at [now], as a row says it: `now`, `4m`, `3h`, `Yesterday`, a weekday
 * within the week, then the date, with the year once it is not this one, by the days of [zone], the
 * phone's own.
 */
fun Instant.ago(now: Instant, zone: ZoneId = ZoneId.systemDefault()): String {
    val seconds = Duration.between(this, now).seconds
    if (seconds < 60) return "now"
    if (seconds < 3600) return "${seconds / 60}m"
    val then = atZone(zone)
    val today = now.atZone(zone).toLocalDate()
    val days = ChronoUnit.DAYS.between(then.toLocalDate(), today)
    val pattern =
        when {
            days == 0L -> return "${seconds / 3600}h"
            days == 1L -> return "Yesterday"
            days < 7 -> "EEE"
            then.year == today.year -> "MMM d"
            else -> "MMM d, yyyy"
        }
    return DateTimeFormatter.ofPattern(pattern, Locale.getDefault()).format(then)
}

/**
 * [this] in full, as an exchange's page says it: `Thu, Nov 5 at 12:29 PM` in [zone], on a 24-hour
 * clock when the phone uses one.
 */
fun Instant.inFull(twentyFourHours: Boolean, zone: ZoneId = ZoneId.systemDefault()): String {
    val time = if (twentyFourHours) "H:mm" else "h:mm a"
    return DateTimeFormatter.ofPattern("EEE, MMM d 'at' $time", Locale.getDefault())
        .format(atZone(zone))
}

/**
 * Who [this] entry is from and where it stands, in words, as [name] reads it and the web's inbox
 * says it: `Alertmanager`, `Claude · You answered: Restart`, or `To Claude · Waiting for an
 * answer`. An open exchange says nothing of its state, since its section already does.
 */
fun InboxEntry.meta(name: String): String {
    val me = "@$name"
    val sent = exchange.from == me
    val parts = mutableListOf<String>()
    if (sent) {
        parts += "To ${exchange.toName ?: exchange.to}"
    } else {
        parts += exchange.fromName ?: exchange.from
        if (exchange.to != me) parts += "to ${exchange.toName ?: exchange.to}"
    }
    standing(me)?.let { parts += it }
    return parts.joinToString(" · ")
}

private fun InboxEntry.standing(me: String): String? {
    if (sending != null) return "Sending your answer…"
    val reply = exchange.reply
    return when (exchange.state) {
        State.OPEN ->
            if (exchange.from == me && exchange.question) "Waiting for an answer" else null
        State.PENDING -> "Waiting to be accepted"
        State.REPLIED -> reply?.let { answered(it, me) } ?: "Answered"
        State.EXPIRED -> if (exchange.question) "Expired unanswered" else "Expired"
        State.WITHDRAWN -> "Withdrawn"
        State.DECLINED -> "Declined"
        State.REPLACED -> "Replaced by a newer message"
    }
}

/** Who answered and what, by the choice's label, or the text when there is no choice. */
private fun InboxEntry.answered(reply: Reply, me: String): String {
    val who = if (reply.by == me) "You" else reply.byName ?: reply.by
    val said =
        reply.choice?.let { key -> exchange.message.choices.find { it.key == key }?.label ?: key }
            ?: reply.text.orEmpty()
    return "$who answered: $said"
}

/**
 * `Alertmanager (@alertmanager) to Chris (@chris) · Thu, Nov 5 at 12:29 PM`, as the web's exchange
 * page begins, with where it stands in words once it is no longer open. With [onGroup], the
 * recipient is a group, and its name is a link that calls it.
 */
@Composable
fun ExchangeMeta(entry: InboxEntry, onGroup: (() -> Unit)?, modifier: Modifier = Modifier) {
    val exchange = entry.exchange
    val link = TextLinkStyles(SpanStyle(MaterialTheme.colorScheme.primary))
    val twentyFourHours = android.text.format.DateFormat.is24HourFormat(LocalContext.current)
    val party = { handle: String, name: String? -> name?.let { "$it ($handle)" } ?: handle }
    MetaText(
        buildAnnotatedString {
            append("${party(exchange.from, exchange.fromName)} to ")
            val to = party(exchange.to, exchange.toName)
            if (onGroup == null) append(to)
            else withLink(LinkAnnotation.Clickable(exchange.to, link) { onGroup() }) { append(to) }
            append(" · ${exchange.created.inFull(twentyFourHours)}")
            entry.copy(sending = null).standingOnPage()?.let { append(" · $it") }
        },
        modifier,
    )
}

/** Where an exchange stands on its own page, which shows the reply itself. */
private fun InboxEntry.standingOnPage(): String? =
    if (exchange.state == State.REPLIED) "Answered" else standing("")
