package sh.transom.inbox

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import sh.transom.R
import sh.transom.pictures.Pictures
import sh.transom.sync.Inbox
import sh.transom.sync.InboxEntry
import sh.transom.theme.ActionButton
import sh.transom.theme.BodyLineHeight
import sh.transom.theme.ButtonKind
import sh.transom.theme.PageHeading
import sh.transom.theme.SectionLabel

/** What the inbox does with what the person taps or swipes. */
class InboxActions(
    /** Opens an exchange by its id. */
    val onOpen: (String) -> Unit,
    /** Marks one exchange read, by its id. */
    val onRead: (String) -> Unit,
    /** Marks each of these exchanges read, as Mark all read does. */
    val onReadAll: (List<String>) -> Unit,
)

/**
 * [name]'s [inbox], as the web's: headed with the `@name`, then the questions waiting on this
 * person, then what they have not read, under Mark all read, then everything else, each newest
 * first. [inbox] is `null` until the phone's copy is first read. [pictures] draws each row's
 * picture.
 */
@Composable
fun InboxScreen(
    name: String,
    inbox: Inbox?,
    pictures: Pictures,
    actions: InboxActions,
    modifier: Modifier = Modifier,
) {
    val empty = stringResource(R.string.inbox_empty)
    val waiting = stringResource(R.string.inbox_waiting)
    val unread = stringResource(R.string.inbox_unread)
    val others = stringResource(R.string.inbox_others)
    val markAll = stringResource(R.string.mark_all_read)
    LazyColumn(modifier, contentPadding = PaddingValues(horizontal = 20.dp, vertical = 24.dp)) {
        item { PageHeading(stringResource(R.string.who, name), Modifier.padding(top = 4.dp)) }
        // The copy is read once the screen is first drawn, and until then the screen shows
        // nothing rather than saying, for a moment, that the inbox is empty.
        val shown = inbox ?: return@LazyColumn
        if (shown.listed.isEmpty()) {
            item {
                Text(
                    empty,
                    Modifier.padding(top = 16.dp),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    lineHeight = BodyLineHeight,
                )
            }
        }
        val row: @Composable (InboxEntry) -> Unit = {
            InboxRow(it, name, pictures, actions.onOpen, actions.onRead)
        }
        section(waiting, shown.waiting, row)
        section(unread, shown.unread, row) {
            ActionButton(markAll, { actions.onReadAll(shown.markable) }, kind = ButtonKind.Quiet)
        }
        section(others, shown.others, row)
    }
}

/**
 * [entries] under [label], with [action] beside the label, ruled above and between, or nothing when
 * there are none.
 */
private fun LazyListScope.section(
    label: String,
    entries: List<InboxEntry>,
    row: @Composable (InboxEntry) -> Unit,
    action: (@Composable () -> Unit)? = null,
) {
    if (entries.isEmpty()) return
    item(key = label) {
        Column {
            Row(
                Modifier.fillMaxWidth().padding(top = 32.dp, bottom = 8.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                SectionLabel(label)
                action?.invoke()
            }
            HorizontalDivider(thickness = 1.dp, color = MaterialTheme.colorScheme.outline)
        }
    }
    items(entries, key = { it.exchange.id }) { row(it) }
}
