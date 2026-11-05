package sh.transom.inbox

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import sh.transom.R
import sh.transom.exchange.ExchangeMeta
import sh.transom.exchange.Message
import sh.transom.exchange.metaLine
import sh.transom.exchange.title
import sh.transom.pictures.PartyPicture
import sh.transom.pictures.Pictures
import sh.transom.sync.InboxEntry
import sh.transom.theme.BodyLineHeight

/**
 * One exchange in [name]'s inbox, as the web's `card.html`. An unread one is full size: the picture
 * of whoever sent it, or for one this person sent, whoever it went to; its title, bold after the
 * accent's dot; the first line of its body; and the meta line. Swiping it to the start marks it
 * read, as the web's Mark read button does. A read one shrinks to one muted line with a smaller,
 * faded picture, so what is unread stands out above it.
 */
@Composable
internal fun InboxRow(
    entry: InboxEntry,
    name: String,
    pictures: Pictures,
    onOpen: (String) -> Unit,
    onRead: (String) -> Unit,
) {
    val exchange = entry.exchange
    val (picture, party) =
        if (exchange.from == "@$name") exchange.toPicture to (exchange.toName ?: exchange.to)
        else exchange.fromPicture to (exchange.fromName ?: exchange.from)
    val open = Modifier.fillMaxWidth().clickable { onOpen(exchange.id) }
    Column {
        if (entry.read == false) {
            Unread(entry, onRead, open) { PartyPicture(pictures, picture, party, 40.dp) }
        } else {
            Row(
                open.padding(horizontal = 4.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                PartyPicture(pictures, picture, party, 28.dp, faded = true)
                val muted = MaterialTheme.colorScheme.onSurfaceVariant
                Text(
                    exchange.message.title,
                    // Each shrinks to fit, the meta line twice as fast, as the web's read row.
                    Modifier.weight(2f, fill = false),
                    color = muted,
                    fontSize = 14.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    metaLine(entry, onGroup = null, handles = false),
                    Modifier.weight(1f, fill = false),
                    color = muted,
                    fontSize = 14.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        HorizontalDivider(thickness = 1.dp, color = MaterialTheme.colorScheme.outline)
    }
}

/**
 * An unread row at full size, which swiping toward the start marks read. A screen reader offers the
 * same as an action, since a swipe is not one it can make.
 */
@Composable
private fun Unread(
    entry: InboxEntry,
    onRead: (String) -> Unit,
    modifier: Modifier = Modifier,
    picture: @Composable () -> Unit,
) {
    val exchange = entry.exchange
    val unreadState = stringResource(R.string.inbox_unread)
    val markRead = stringResource(R.string.mark_read)
    val state = rememberSwipeToDismissBoxState()
    SwipeToDismissBox(
        state,
        backgroundContent = {
            Box(
                Modifier.fillMaxSize().background(MaterialTheme.colorScheme.primary),
                Alignment.CenterEnd,
            ) {
                Text(
                    markRead,
                    Modifier.padding(horizontal = 20.dp),
                    color = MaterialTheme.colorScheme.onPrimary,
                    fontSize = 14.sp,
                )
            }
        },
        enableDismissFromStartToEnd = false,
        onDismiss = { if (it == SwipeToDismissBoxValue.EndToStart) onRead(exchange.id) },
    ) {
        Row(
            modifier
                .background(MaterialTheme.colorScheme.surface)
                .semantics {
                    stateDescription = unreadState
                    customActions =
                        listOf(
                            CustomAccessibilityAction(markRead) {
                                onRead(exchange.id)
                                true
                            }
                        )
                }
                .padding(horizontal = 4.dp, vertical = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            picture()
            Column(Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(
                        Modifier.padding(end = 8.dp)
                            .size(7.dp)
                            .background(MaterialTheme.colorScheme.primary, CircleShape)
                    )
                    Text(
                        exchange.message.title,
                        fontWeight = FontWeight.SemiBold,
                        lineHeight = BodyLineHeight,
                    )
                }
                val excerpt = exchange.message.excerpt
                if (excerpt.isNotEmpty()) {
                    Text(
                        excerpt,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                ExchangeMeta(entry, onGroup = null)
            }
        }
    }
}

/**
 * The first line of a message's body, to show under its summary in a list, cut at 120 characters,
 * or nothing when the body's first line is already its title, as the web makes it.
 */
internal val Message.excerpt: String
    get() {
        if (summary.isNullOrEmpty()) return ""
        val line = body.orEmpty().lineSequence().firstOrNull { it.isNotBlank() }.orEmpty()
        val characters = line.codePoints().toArray()
        return if (characters.size <= EXCERPT) line else String(characters, 0, EXCERPT) + "…"
    }

private const val EXCERPT = 120
