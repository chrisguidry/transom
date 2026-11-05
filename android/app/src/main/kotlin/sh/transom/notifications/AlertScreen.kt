package sh.transom.notifications

import android.graphics.Typeface
import android.text.format.DateFormat
import android.text.format.DateUtils
import android.widget.TextClock
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import java.time.Duration
import java.util.Date
import kotlinx.coroutines.flow.map
import sh.transom.Account
import sh.transom.R
import sh.transom.exchange.State
import sh.transom.exchange.title
import sh.transom.pictures.PartyPicture
import sh.transom.sync.InboxEntry

/**
 * Exchange [id] as the full-screen page shows it, in the colors of an alarm whatever the theme: the
 * time, the sender's picture, who sent it and how long ago, the summary, and the body; then, within
 * reach of a thumb, when it rings again, the question's choices and the message's first link, Got
 * it, and Open in transom. [onDone] closes the page once the exchange is no longer an open, unread
 * message waiting on this person.
 */
@Composable
internal fun AlertScreen(
    account: Account,
    id: String,
    onAcknowledge: () -> Unit,
    onChoose: (String) -> Unit,
    onLink: (String) -> Unit,
    onOpen: () -> Unit,
    onDone: () -> Unit,
) {
    val loaded by
        remember(account, id) { account.store.exchange(id).map { Loaded(it) } }
            .collectAsStateWithLifecycle(null)
    val entry = (loaded ?: return).entry
    val waiting =
        entry != null &&
            entry.exchange.state == State.OPEN &&
            entry.read == false &&
            entry.sending == null
    LaunchedEffect(waiting) { if (!waiting) onDone() }
    val exchange = (entry ?: return).exchange
    val message = exchange.message
    val markdown = remember(account) { notificationMarkdown(account.client::resolve) }
    val body = remember(message) { notificationText(message.copy(summary = null), markdown) }
    val name = exchange.fromName ?: exchange.from
    val context = LocalContext.current
    val arrived =
        DateUtils.getRelativeTimeSpanString(
            exchange.created.toEpochMilli(),
            System.currentTimeMillis(),
            DateUtils.MINUTE_IN_MILLIS,
        )
    // The page says when it rings again, until the last repeat has rung.
    val next =
        REPEATS.getOrNull(exchange.repeats)?.let {
            DateFormat.getTimeFormat(context).format(Date.from(exchange.created.plus(it)))
        }
    // A long body reads more easily from the left than centered.
    var long by remember(body) { mutableStateOf(false) }
    Column(
        Modifier.fillMaxSize()
            .background(Brush.verticalGradient(listOf(TOP, BOTTOM)))
            .windowInsetsPadding(WindowInsets.safeDrawing)
            .padding(horizontal = 28.dp, vertical = 32.dp)
    ) {
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Clock()
            PartyPicture(
                account.pictures,
                exchange.fromPicture,
                name,
                64.dp,
                Modifier.padding(top = 16.dp),
            )
            Text("$name · $arrived", color = WHY, fontSize = 14.sp)
            Text(
                message.title,
                Modifier.semantics { heading() },
                color = INK,
                fontSize = 22.sp,
                fontWeight = FontWeight.SemiBold,
                textAlign = TextAlign.Center,
            )
            if (body.isNotEmpty()) {
                Text(
                    body.toString(),
                    Modifier.fillMaxWidth(),
                    color = WHY,
                    fontSize = 15.sp,
                    textAlign = if (long) TextAlign.Start else TextAlign.Center,
                    onTextLayout = { long = it.lineCount > 2 },
                )
            }
        }
        // The buttons sit in the bottom third, in reach of a thumb, with Got it widest.
        Column(
            Modifier.fillMaxWidth().padding(top = 16.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            next?.let {
                Text(stringResource(R.string.rings_again, it), color = WHY, fontSize = 13.sp)
            }
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.CenterHorizontally),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                if (exchange.question) {
                    message.choices.forEach { PageButton(it.label) { onChoose(it.key) } }
                }
                message.links.firstOrNull()?.let { link ->
                    PageButton(link.title ?: stringResource(R.string.open_link)) {
                        onLink(link.href)
                    }
                }
            }
            Text(
                stringResource(R.string.acknowledge),
                Modifier.fillMaxWidth()
                    .heightIn(min = 56.dp)
                    .background(ACK, RoundedCornerShape(50))
                    .semantics { role = Role.Button }
                    .clickable(onClick = onAcknowledge)
                    .padding(vertical = 16.dp),
                color = ACK_INK,
                fontSize = 17.sp,
                fontWeight = FontWeight.SemiBold,
                textAlign = TextAlign.Center,
            )
            Text(
                stringResource(R.string.open_in_transom),
                Modifier.semantics { role = Role.Button }
                    .clickable(onClick = onOpen)
                    .heightIn(min = 48.dp)
                    .padding(horizontal = 12.dp, vertical = 14.dp),
                color = WHY,
                fontSize = 14.sp,
            )
        }
    }
}

/** How long after an urgent message arrives each of its repeats rings, as the server sends them. */
private val REPEATS = listOf(5L, 15L, 30L).map { Duration.ofMinutes(it) }

/** An exchange once the copy has been read: [entry] is `null` when the copy has none. */
private class Loaded(val entry: InboxEntry?)

/**
 * The time, as the lock screen the page covers shows it. Android's own clock keeps it current and
 * follows the phone's choice of a 12- or 24-hour clock.
 */
@Composable
private fun Clock() {
    AndroidView({ context ->
        TextClock(context).apply {
            setTextColor(INK.toArgb())
            textSize = 52f
            typeface = Typeface.create("sans-serif-light", Typeface.NORMAL)
        }
    })
}

/** One of the page's secondary buttons, a choice or a link, 48 dp tall. */
@Composable
private fun PageButton(label: String, onClick: () -> Unit) {
    Text(
        label,
        Modifier.background(BUTTON, RoundedCornerShape(50))
            .semantics { role = Role.Button }
            .clickable(onClick = onClick)
            .heightIn(min = 48.dp)
            .padding(horizontal = 20.dp, vertical = 14.dp),
        color = INK,
        fontSize = 15.sp,
    )
}

// The page's colors, from the design's lock-screen page: a dark red that reads as an alarm in a
// dark room, in light and dark themes alike.
private val TOP = Color(0xFF2B1B1A)
private val BOTTOM = Color(0xFF120D0D)
private val INK = Color(0xFFFBEEEC)
private val WHY = Color(0xFFE7C9C4)
private val BUTTON = Color(0xFF3C2725)
private val ACK = Color(0xFFFFB4AB)
private val ACK_INK = Color(0xFF2B0B07)
