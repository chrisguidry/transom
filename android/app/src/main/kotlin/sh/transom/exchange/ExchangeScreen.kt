package sh.transom.exchange

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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.launch
import sh.transom.Account
import sh.transom.R
import sh.transom.sync.InboxEntry
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
 * Opening it reads it, as the web's page does, and a reply goes through the outbox: it shows as
 * sending at once, and the server's answer settles it, with someone else's reply when theirs came
 * first.
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
    val scope = rememberCoroutineScope()
    val markdown = rememberMarkdown(account.client::resolve)
    val body =
        remember(markdown, message) { markdown.body(message.body.orEmpty(), message.attachments) }
    val open = exchange.state == State.OPEN
    Column(
        modifier.verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 24.dp)
    ) {
        // Only a recipient has a read state, so a recipient other than this person is a group
        // they are a member of.
        val group = exchange.to.takeIf { shown.read != null && it != "@${account.name}" }
        ExchangeMeta(shown, group?.let { { onGroup(it.removePrefix("@")) } })
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
            val reply = shown.shownReply(account.name)
            if (reply != null) {
                ReplySection(reply)
            } else if (open && shown.read != null) {
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
 * The reply [this] entry shows: the one this phone is sending, or the exchange's own. Each names
 * its choice by the choice's label, as the web does.
 */
private fun InboxEntry.shownReply(name: String): ShownReply? {
    val label = { key: String -> exchange.message.choices.find { it.key == key }?.label ?: key }
    sending?.let {
        return ShownReply("@$name", "sending", it.choice?.let(label), it.text)
    }
    return exchange.reply?.let { ShownReply(it.by, it.at.shown(), it.choice?.let(label), it.text) }
}

/** The message's links, as the web's `.links`: each its relation and address, or its address. */
@Composable
private fun Links(links: List<Link>, resolve: (String) -> String) {
    val style =
        TextLinkStyles(
            SpanStyle(MaterialTheme.colorScheme.primary, textDecoration = TextDecoration.Underline)
        )
    Column {
        links.forEach { link ->
            Row {
                Text("•", Modifier.width(18.dp), lineHeight = BodyLineHeight)
                Text(
                    buildAnnotatedString {
                        withLink(LinkAnnotation.Url(resolve(link.href), style)) {
                            append(link.rel?.let { "$it: ${link.href}" } ?: link.href)
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
