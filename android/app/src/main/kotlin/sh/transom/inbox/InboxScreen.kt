package sh.transom.inbox

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import sh.transom.R
import sh.transom.pictures.Pictures
import sh.transom.sync.Inbox
import sh.transom.sync.Listing
import sh.transom.theme.BodyLineHeight
import sh.transom.theme.PageHeading
import sh.transom.theme.SectionLabel

/** What the inbox does with what the person taps or swipes. */
class InboxActions(
    /** Opens an exchange by its id. */
    val onOpen: (String) -> Unit,
    /** Marks these exchanges read. */
    val onRead: (List<String>) -> Unit,
    /** Marks these exchanges unread again, as Undo does. */
    val onUnread: (List<String>) -> Unit,
    /** Answers an exchange by its id with a choice's key. */
    val onAnswer: (String, String) -> Unit,
)

/**
 * [name]'s [inbox], as the web's: headed with the `@name`, then what waits on this person, what
 * they have not read, under Mark all read, and everything earlier, each newest first, and the older
 * rows behind "Show older". [inbox] is `null` until the phone's copy is first read. [pictures]
 * draws each row's picture. Marking read, by a swipe or by Mark all read, offers Undo for six
 * seconds.
 */
@Composable
fun InboxScreen(
    name: String,
    inbox: Inbox?,
    pictures: Pictures,
    actions: InboxActions,
    modifier: Modifier = Modifier,
) {
    val snackbar = remember { SnackbarHostState() }
    val scope = rememberCoroutineScope()
    val resources = LocalResources.current
    val undo = stringResource(R.string.undo)
    val undoable =
        remember(actions) {
            InboxActions(
                actions.onOpen,
                onRead = { ids ->
                    actions.onRead(ids)
                    scope.launch {
                        val said =
                            resources.getQuantityString(R.plurals.marked_read, ids.size, ids.size)
                        val result =
                            withTimeoutOrNull(UNDO) {
                                snackbar.showSnackbar(
                                    said,
                                    undo,
                                    duration = SnackbarDuration.Indefinite,
                                )
                            }
                        if (result == SnackbarResult.ActionPerformed) actions.onUnread(ids)
                    }
                },
                actions.onUnread,
                actions.onAnswer,
            )
        }
    Box(modifier) {
        Sections(name, inbox, pictures, undoable, Modifier.fillMaxSize())
        SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter))
    }
}

private val UNDO = 6.seconds

@Composable
private fun Sections(
    name: String,
    inbox: Inbox?,
    pictures: Pictures,
    actions: InboxActions,
    modifier: Modifier,
) {
    val waiting = stringResource(R.string.inbox_waiting, inbox?.waiting?.size ?: 0)
    val unread = stringResource(R.string.inbox_unread_count, inbox?.unread?.size ?: 0)
    val earlier = stringResource(R.string.inbox_earlier)
    val markAll = stringResource(R.string.mark_all_read)
    var older by rememberSaveable { mutableStateOf(false) }
    val showOlder =
        pluralStringResource(
            R.plurals.inbox_older,
            inbox?.older?.size ?: 0,
            inbox?.older?.size ?: 0,
        )
    LazyColumn(modifier, contentPadding = PaddingValues(horizontal = 20.dp, vertical = 24.dp)) {
        item { PageHeading(stringResource(R.string.who, name), Modifier.padding(top = 4.dp)) }
        // The copy is read once the screen is first drawn, and until then the screen shows
        // nothing rather than saying, for a moment, that the inbox is empty.
        val shown = inbox ?: return@LazyColumn
        val note =
            when {
                shown.isEmpty -> R.string.inbox_empty
                shown.waiting.isEmpty() && shown.unread.isEmpty() -> R.string.inbox_caught_up
                else -> null
            }
        note?.let { item { Note(stringResource(it, name)) } }
        section(waiting, shown.waiting.map { Listing(listOf(it)) }) {
            InboxRow(it, name, waiting = true, pictures, shown.now, actions)
        }
        section(unread, shown.unread, { MarkAll(markAll) { actions.onRead(shown.markable) } }) {
            InboxRow(it, name, waiting = false, pictures, shown.now, actions)
        }
        section(earlier, shown.earlier) {
            InboxRow(it, name, waiting = false, pictures, shown.now, actions)
        }
        if (shown.older.isNotEmpty()) {
            if (older) {
                items(shown.older, key = { it.entry.exchange.id }) {
                    InboxRow(it, name, waiting = false, pictures, shown.now, actions)
                }
            } else {
                item {
                    Text(
                        showOlder,
                        Modifier.fillMaxWidth()
                            .clickable { older = true }
                            .heightIn(min = 48.dp)
                            .padding(vertical = 14.dp),
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        fontSize = 14.sp,
                    )
                }
            }
        }
    }
}

/** A muted line at the top of the inbox, when there is nothing to do. */
@Composable
private fun Note(text: String) {
    Text(
        text,
        Modifier.padding(top = 16.dp),
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        lineHeight = BodyLineHeight,
    )
}

/** Mark all read, as a text button in the accent beside its section's label. */
@Composable
private fun MarkAll(text: String, onClick: () -> Unit) {
    Text(
        text,
        Modifier.clickable(onClick = onClick)
            .heightIn(min = 48.dp)
            .padding(horizontal = 8.dp, vertical = 14.dp),
        color = MaterialTheme.colorScheme.primary,
        fontSize = 14.sp,
        fontWeight = FontWeight.Medium,
    )
}

/**
 * [listings] under [label], with [action] beside the label, ruled above and between, or nothing
 * when there are none.
 */
private fun LazyListScope.section(
    label: String,
    listings: List<Listing>,
    action: (@Composable () -> Unit)? = null,
    row: @Composable (Listing) -> Unit,
) {
    if (listings.isEmpty()) return
    item(key = label.substringBefore(" · ")) {
        Column {
            Row(
                Modifier.fillMaxWidth().padding(top = 24.dp, bottom = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                SectionLabel(
                    label,
                    Modifier.weight(1f).semantics {
                        contentDescription = label.replace(" · ", ", ")
                    },
                )
                action?.invoke()
            }
            HorizontalDivider(thickness = 1.dp, color = MaterialTheme.colorScheme.outline)
        }
    }
    items(listings, key = { it.entry.exchange.id }) { row(it) }
}
