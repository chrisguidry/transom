package sh.transom.inbox

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import sh.transom.R
import sh.transom.exchange.ExchangeMeta
import sh.transom.exchange.title
import sh.transom.sync.Inbox
import sh.transom.sync.InboxEntry
import sh.transom.theme.BodyLineHeight
import sh.transom.theme.PageHeading
import sh.transom.theme.SectionLabel

/**
 * [name]'s [inbox], as the web's: headed with the `@name`, then the questions waiting on this
 * person, then everything else, each newest first. [inbox] is `null` until the phone's copy is
 * first read, and [onOpen] opens an exchange by its id.
 */
@Composable
fun InboxScreen(
    name: String,
    inbox: Inbox?,
    onOpen: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val empty = stringResource(R.string.inbox_empty)
    val waiting = stringResource(R.string.inbox_waiting)
    val others = stringResource(R.string.inbox_others)
    LazyColumn(modifier, contentPadding = PaddingValues(horizontal = 20.dp, vertical = 24.dp)) {
        item { PageHeading(stringResource(R.string.who, name), Modifier.padding(top = 4.dp)) }
        // The copy is read once the screen is first drawn, and until then the screen shows
        // nothing rather than saying, for a moment, that the inbox is empty.
        val shown = inbox ?: return@LazyColumn
        if (shown.entries.isEmpty()) {
            item {
                Text(
                    empty,
                    Modifier.padding(top = 16.dp),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    lineHeight = BodyLineHeight,
                )
            }
        }
        section(waiting, shown.waiting, onOpen)
        section(others, shown.others, onOpen)
    }
}

/** [entries] under [label], ruled above and between, or nothing when there are none. */
private fun LazyListScope.section(
    label: String,
    entries: List<InboxEntry>,
    onOpen: (String) -> Unit,
) {
    if (entries.isEmpty()) return
    item(key = label) {
        Column {
            SectionLabel(label, Modifier.padding(top = 32.dp, bottom = 8.dp))
            HorizontalDivider(thickness = 1.dp, color = MaterialTheme.colorScheme.outline)
        }
    }
    items(entries, key = { it.exchange.id }) { Card(it, onOpen) }
}

/**
 * One exchange, as the web's `card.html`: its title, bold after the accent's dot while it is
 * unread, over the meta line.
 */
@Composable
private fun Card(entry: InboxEntry, onOpen: (String) -> Unit) {
    val unread = entry.read == false
    val unreadState = stringResource(R.string.inbox_unread)
    Column {
        Column(
            Modifier.fillMaxWidth()
                .clickable { onOpen(entry.exchange.id) }
                .semantics { if (unread) stateDescription = unreadState }
                .padding(horizontal = 4.dp, vertical = 12.dp)
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (unread) {
                    Box(
                        Modifier.padding(end = 8.dp)
                            .size(7.dp)
                            .background(MaterialTheme.colorScheme.primary, CircleShape)
                    )
                }
                Text(
                    entry.exchange.message.title,
                    fontWeight = if (unread) FontWeight.SemiBold else null,
                    lineHeight = BodyLineHeight,
                )
            }
            ExchangeMeta(entry)
        }
        HorizontalDivider(thickness = 1.dp, color = MaterialTheme.colorScheme.outline)
    }
}
