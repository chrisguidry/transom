package sh.transom.exchange

import android.text.format.DateFormat
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.launch
import sh.transom.Account
import sh.transom.R
import sh.transom.pictures.Badged
import sh.transom.pictures.PartyPicture
import sh.transom.sync.Sending
import sh.transom.theme.ActionButton
import sh.transom.theme.BodyLineHeight
import sh.transom.theme.ButtonKind
import sh.transom.theme.PageHeading

/**
 * Exchange [id] as [account] sees it, as the web's `exchange.html`: the meta line, the title, the
 * body, the images it did not place, the links, and then the reply, or the answer for a recipient
 * while it is open. Its sender can withdraw it while it is open or pending.
 *
 * When it came to a group the person is in, the group's name in the meta line opens the group's
 * members with [onGroup], which takes the name without its `@`.
 *
 * Opening it reads it, as the web's page does, and a reply goes through the outbox: it takes the
 * answer's place at once, as not sent yet, and the server's answer settles it. When someone else's
 * reply came first, this phone's own shows over theirs, as not used, until the person leaves the
 * screen.
 */
@Composable
fun ExchangeScreen(
    account: Account,
    id: String,
    onGroup: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val entry by
        remember(account, id) { account.store.exchange(id) }.collectAsStateWithLifecycle(null)
    val shown = entry ?: return
    val exchange = shown.exchange
    val message = exchange.message
    LaunchedEffect(id, shown.read) { if (shown.read == false) account.outbox.mark(id, read = true) }
    // Leaving the screen is having seen that an answer was not used. The screen also leaves when
    // the activity is made again, as when the phone turns, and comes back to the same exchange, so
    // it counts only while the activity is on screen.
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val lost by rememberUpdatedState(shown.lost != null)
    DisposableEffect(account, id) {
        onDispose {
            if (lost && lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) {
                account.background { account.store.seen(id) }
            }
        }
    }
    val scope = rememberCoroutineScope()
    val markdown = rememberMarkdown(account.client::resolve)
    val body =
        remember(markdown, message) { markdown.body(message.body.orEmpty(), message.attachments) }
    val open = exchange.state == State.OPEN
    Column(
        modifier
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp, vertical = 24.dp),
    ) {
        // Only a recipient has a read state, so a recipient other than this person is a group
        // they are a member of.
        val group = exchange.to.takeIf { shown.read != null && it != "@${account.name}" }
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Badged(exchange.disposition, 18.dp) {
                PartyPicture(
                    account.pictures,
                    exchange.fromPicture,
                    exchange.fromName ?: exchange.from,
                    40.dp,
                )
            }
            ExchangeMeta(shown, group?.let { { onGroup(it.removePrefix("@")) } })
        }
        PageHeading(message.title, Modifier.padding(top = 4.dp, bottom = 16.dp))
        // The web's body, images, links, reply, and answer each end up 24 px from the next once
        // their margins collapse, so one spacing between the sections stands for all of them.
        Column(verticalArrangement = Arrangement.spacedBy(24.dp)) {
            if (body.blocks.isNotEmpty()) {
                MarkdownBlocks(
                    body.blocks,
                    picture = { AttachedImage(account.client, id, it.attachment, it.alt) },
                )
            }
            val rest = message.attachments.filter { it.name !in body.placed }
            if (rest.isNotEmpty()) {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    rest.forEach { AttachedImage(account.client, id, it, it.name) }
                }
            }
            if (message.links.isNotEmpty()) Links(message.links, account.client::resolve)
            val twentyFourHours = DateFormat.is24HourFormat(LocalContext.current)
            val said = { reply: Sending -> exchange.said(reply.choice, reply.text) }
            val sending = shown.sending
            if (sending != null) {
                OwnReply(
                    stringResource(R.string.reply_unsent),
                    said(sending),
                    stringResource(R.string.reply_unsent_note),
                )
            }
            val reply = exchange.reply
            val mine = shown.lost
            if (mine != null && reply != null) {
                OwnReply(
                    stringResource(R.string.reply_lost),
                    said(mine),
                    exchange.notUsed(mine, reply),
                    lost = true,
                )
            }
            if (reply != null) {
                ReplySection(
                    ShownReply(
                        reply.byName ?: reply.by,
                        reply.at.inFull(twentyFourHours),
                        reply.choice?.let { exchange.said(it, null) },
                        reply.text,
                    ),
                )
            } else if (open && shown.read != null && sending == null) {
                // Only a recipient has a read state, so only a recipient answers.
                AnswerForm(
                    message,
                    markdown,
                    onReply = { choice, text ->
                        scope.launch { account.outbox.reply(id, choice, text) }
                    },
                )
            }
            if (exchange.from == "@${account.name}" && (open || exchange.state == State.PENDING)) {
                Withdraw { account.withdraw(id) }
            }
        }
    }
}

/**
 * The message's links, as the web's `.links`: each by its title, or its relation and address, or
 * its address.
 */
@Composable
private fun Links(links: List<Link>, resolve: (String) -> String) {
    val style =
        TextLinkStyles(
            SpanStyle(MaterialTheme.colorScheme.primary, textDecoration = TextDecoration.Underline),
        )
    Column {
        links.forEach { link ->
            Row {
                Text("•", Modifier.width(18.dp), lineHeight = BodyLineHeight)
                Text(
                    buildAnnotatedString {
                        withLink(LinkAnnotation.Url(resolve(link.href), style)) {
                            append(link.title ?: link.rel?.let { "$it: ${link.href}" } ?: link.href)
                        }
                    },
                    lineHeight = BodyLineHeight,
                )
            }
        }
    }
}

/**
 * The quiet Withdraw button, and what went wrong when [withdraw] returns `false`. A withdrawal is
 * sent at once rather than queued: a question withdrawn offline could be answered meanwhile, and
 * the sender should know at once whether it was withdrawn.
 */
@Composable
private fun Withdraw(withdraw: suspend () -> Boolean) {
    val scope = rememberCoroutineScope()
    var failed by remember { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        ActionButton(
            stringResource(R.string.withdraw),
            { scope.launch { failed = !withdraw() } },
            kind = ButtonKind.Quiet,
        )
        if (failed) {
            Text(
                stringResource(R.string.withdraw_failed),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                lineHeight = BodyLineHeight,
            )
        }
    }
}
