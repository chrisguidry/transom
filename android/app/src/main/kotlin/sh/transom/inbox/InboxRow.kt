package sh.transom.inbox

import android.text.format.DateUtils
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.Text
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import java.time.Instant
import sh.transom.R
import sh.transom.exchange.Urgency
import sh.transom.exchange.ago
import sh.transom.exchange.excerpt
import sh.transom.exchange.meta
import sh.transom.exchange.title
import sh.transom.pictures.PartyPicture
import sh.transom.pictures.Pictures
import sh.transom.sync.InboxEntry
import sh.transom.sync.Listing
import sh.transom.theme.ActionButton
import sh.transom.theme.ButtonKind
import sh.transom.theme.DarkPalette
import sh.transom.theme.LightPalette

/**
 * One row of [name]'s inbox, as the web's `card.html`. A row that is unread, or that waits on the
 * person, is full size: the picture of whoever sent it, or for one they sent, whoever it went to,
 * with the urgency's dot while unread; the title, bold, with the time beside it; the body's first
 * line; who it is from and where it stands; and the choices that answer it in a tap. Swiping an
 * unread row toward the start marks it read. Any other row is two quiet lines and a smaller, faded
 * picture, so what is unread stands out above it.
 */
@Composable
internal fun InboxRow(
    listing: Listing,
    name: String,
    waiting: Boolean,
    pictures: Pictures,
    now: Instant,
    actions: InboxActions,
) {
    val entry = listing.entry
    val exchange = entry.exchange
    val (picture, party) =
        if (exchange.from == "@$name") exchange.toPicture to (exchange.toName ?: exchange.to)
        else exchange.fromPicture to (exchange.fromName ?: exchange.from)
    val unread = entry.read == false
    val times = listing.entries.size.takeIf { it > 1 }
    val meta = entry.meta(name) + times?.let { " · $it times" }.orEmpty()
    val open = Modifier.fillMaxWidth().clickable { actions.onOpen(exchange.id) }
    Column {
        if (unread || waiting) {
            val full: @Composable (Modifier) -> Unit = { modifier ->
                Full(listing, name, waiting, meta, now, modifier, actions) {
                    Box {
                        PartyPicture(pictures, picture, party, 40.dp)
                        dotColor(exchange.message.urgency)
                            .takeIf { unread }
                            ?.let {
                                Dot(it, Modifier.align(Alignment.BottomEnd).offset(2.dp, 2.dp))
                            }
                    }
                }
            }
            if (unread) Swiped(listing.ids, actions, open, full) else full(open)
        } else {
            Quiet(entry, meta, now, open) {
                PartyPicture(pictures, picture, party, 32.dp, faded = true)
            }
        }
        HorizontalDivider(thickness = 1.dp, color = MaterialTheme.colorScheme.outline)
    }
}

/**
 * A row at full size: three lines beside the picture, and the choices that answer it under them.
 */
@Composable
private fun Full(
    listing: Listing,
    name: String,
    waiting: Boolean,
    meta: String,
    now: Instant,
    modifier: Modifier,
    actions: InboxActions,
    picture: @Composable () -> Unit,
) {
    val entry = listing.entry
    val exchange = entry.exchange
    val unread = entry.read == false
    val state =
        stringResource(
            when {
                unread -> R.string.inbox_unread
                else -> R.string.inbox_needs_answer
            }
        )
    val markRead = stringResource(R.string.mark_read)
    val excerpt = exchange.message.excerpt
    val spoken = spoken(exchange.created, now)
    Column(modifier.background(MaterialTheme.colorScheme.surface)) {
        Row(
            Modifier.semantics {
                    stateDescription = state
                    contentDescription =
                        listOf(state, "$meta, $spoken", exchange.message.title, excerpt)
                            .filter { it.isNotEmpty() }
                            // Each part a sentence, without a second stop after a title's own.
                            .joinToString(" ") { if (it.last() in ".?!…") it else "$it." }
                    if (unread) {
                        customActions =
                            listOf(
                                CustomAccessibilityAction(markRead) {
                                    actions.onRead(listing.ids)
                                    true
                                }
                            )
                    }
                }
                .padding(horizontal = 4.dp, vertical = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            picture()
            Column(Modifier.weight(1f)) {
                TitleLine(exchange.message.title, exchange.created.ago(now), bold = true, unread)
                if (excerpt.isNotEmpty()) {
                    Text(
                        excerpt,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        fontSize = 14.sp,
                        maxLines = if (waiting) 1 else 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Text(
                    meta,
                    color =
                        if (entry.sending != null) MaterialTheme.colorScheme.primary
                        else MaterialTheme.colorScheme.onSurfaceVariant,
                    fontSize = 13.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        val choices = entry.rowChoices(name)
        if (choices.isNotEmpty()) {
            FlowRow(
                Modifier.padding(start = 56.dp, end = 4.dp, bottom = 12.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                choices.forEach { choice ->
                    ActionButton(
                        choice.label,
                        { actions.onAnswer(exchange.id, choice.key) },
                        Modifier.heightIn(min = 36.dp),
                        kind = ButtonKind.Default,
                    )
                }
            }
        }
    }
}

/** A read row: the title with its time, then who it is from, quiet and two lines tall. */
@Composable
private fun Quiet(
    entry: InboxEntry,
    meta: String,
    now: Instant,
    modifier: Modifier,
    picture: @Composable () -> Unit,
) {
    val read = stringResource(R.string.inbox_read)
    Row(
        modifier
            .semantics { stateDescription = read }
            .heightIn(min = 60.dp)
            .padding(horizontal = 4.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        picture()
        Column(Modifier.weight(1f)) {
            TitleLine(entry.exchange.message.title, entry.exchange.created.ago(now), bold = false)
            Text(
                meta,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                fontSize = 13.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/** The title, after the accent's dot while [unread], with the time right-aligned beside it. */
@Composable
private fun TitleLine(title: String, time: String, bold: Boolean, unread: Boolean = false) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        if (unread) {
            Box(
                Modifier.padding(end = 8.dp)
                    .size(7.dp)
                    .background(MaterialTheme.colorScheme.primary, CircleShape)
            )
        }
        Text(
            title,
            Modifier.weight(1f),
            color = if (bold) Color.Unspecified else MaterialTheme.colorScheme.onSurfaceVariant,
            fontSize = if (bold) 16.sp else 15.sp,
            fontWeight = if (bold) FontWeight.SemiBold else null,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            time,
            Modifier.padding(start = 8.dp),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            style = TextStyle(fontSize = 13.sp, fontFeatureSettings = "tnum"),
        )
    }
}

/**
 * An unread row that swiping toward the start marks read, over the accent with a check and Mark
 * read. A screen reader offers the same as an action, since a swipe is not one it can make.
 */
@Composable
private fun Swiped(
    ids: List<String>,
    actions: InboxActions,
    modifier: Modifier,
    row: @Composable (Modifier) -> Unit,
) {
    val state = rememberSwipeToDismissBoxState()
    SwipeToDismissBox(
        state,
        backgroundContent = {
            Row(
                Modifier.fillMaxSize()
                    .background(MaterialTheme.colorScheme.primary)
                    .padding(horizontal = 20.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    painterResource(R.drawable.ic_check),
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onPrimary,
                )
                Text(
                    stringResource(R.string.mark_read),
                    color = MaterialTheme.colorScheme.onPrimary,
                    fontSize = 14.sp,
                )
            }
        },
        enableDismissFromStartToEnd = false,
        onDismiss = { actions.onRead(ids) },
    ) {
        row(modifier)
    }
}

/** The urgency's dot, ringed in the paper's color so red still shows on an orange picture. */
@Composable
private fun Dot(color: Color, modifier: Modifier) {
    Box(
        modifier
            .size(12.dp)
            .background(color, CircleShape)
            .border(2.5.dp, MaterialTheme.colorScheme.surface, CircleShape)
    )
}

/** The dot an unread row's picture carries: red for urgent and high, amber for normal. */
@Composable
private fun dotColor(urgency: Urgency): Color? {
    val palette = if (isSystemInDarkTheme()) DarkPalette else LightPalette
    return when (urgency) {
        Urgency.URGENT,
        Urgency.HIGH -> palette.fire
        Urgency.NORMAL -> palette.warn
        Urgency.LOW,
        Urgency.VERY_LOW -> null
    }
}

/** How long ago [at] was, as a screen reader says it, such as "5 minutes ago". */
private fun spoken(at: Instant, now: Instant): String =
    DateUtils.getRelativeTimeSpanString(
            at.toEpochMilli(),
            now.toEpochMilli(),
            DateUtils.MINUTE_IN_MILLIS,
        )
        .toString()
