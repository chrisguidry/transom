package sh.transom.notifications

import android.app.Notification
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat
import androidx.core.app.Person
import androidx.core.app.RemoteInput
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat
import androidx.core.net.toUri
import java.time.Instant
import sh.transom.MainActivity
import sh.transom.R
import sh.transom.exchange.Exchange
import sh.transom.exchange.Reply
import sh.transom.settings.Preferences

/**
 * Makes the notification of each [Shown] exchange. Each sender is a conversation: the notification
 * is `MessagingStyle`, from the sender as a [Person], with a long-lived shortcut of the sender's
 * own, so Android's long-press menu lets the person make one sender a priority conversation or
 * silence it.
 */
internal class Posting(private val context: Context, private val resolve: (String) -> String) {
    private val markdown = notificationMarkdown(resolve)
    private val me = Person.Builder().setName(context.getString(R.string.me)).setKey(ME).build()
    private val shortcuts = mutableSetOf<String>()
    private val preferences = Preferences(context)

    /** The notification [shown] makes, posted without a sound when [silent], at [now]. */
    fun notification(shown: Shown, silent: Boolean, now: Instant): Notification {
        val exchange = shown.entry.exchange
        val sender = person(exchange.from)
        shortcut(sender)
        val style =
            NotificationCompat.MessagingStyle(me)
                .addMessage(
                    notificationText(exchange.message, markdown),
                    exchange.created.toEpochMilli(),
                    sender,
                )
        when (shown) {
            is Shown.Asking -> {}
            is Shown.Sent -> style.addMessage(shown.said, now.toEpochMilli(), null as Person?)
            is Shown.Answered -> answered(style, exchange, shown.reply)
        }
        val builder =
            NotificationCompat.Builder(context, Channels.of(exchange.message.urgency))
                .setSmallIcon(R.drawable.ic_notification)
                .setStyle(style)
                .setShortcutId(sender.key)
                .setCategory(NotificationCompat.CATEGORY_MESSAGE)
                .setWhen(exchange.created.toEpochMilli())
                .setShowWhen(true)
                .setOnlyAlertOnce(true)
                .setSilent(silent || shown !is Shown.Asking)
                .setAutoCancel(true)
                .setAllowSystemGeneratedContextualActions(false)
                .setContentIntent(open(exchange.id))
                .setDeleteIntent(broadcast(NotificationReceiver.DISMISS, exchange.id))
        exchange.expires?.let {
            builder.setTimeoutAfter((it.toEpochMilli() - now.toEpochMilli()).coerceAtLeast(1))
        }
        if (shown is Shown.Asking) actions(builder, exchange)
        return builder.build()
    }

    /**
     * Reply first, since Android shows three actions at most and drops the rest: a reply box
     * labelled with the question's prompt, with the choices as chips when they all fit. Then Open,
     * and the message's first link. A chip or the reply box always needs an unlocked phone, and the
     * plain actions ask for one too unless the person lifted that in Settings.
     */
    private fun actions(builder: NotificationCompat.Builder, exchange: Exchange) {
        val message = exchange.message
        val label = message.prompt?.takeIf { it.isNotEmpty() } ?: context.getString(R.string.reply)
        val input =
            RemoteInput.Builder(NotificationReceiver.ANSWER)
                .setLabel(label)
                .setChoices(message.chips.map { it.label }.toTypedArray())
                .setAllowFreeFormInput(true)
                .build()
        builder.addAction(
            NotificationCompat.Action.Builder(
                    icon,
                    context.getString(R.string.reply),
                    broadcast(NotificationReceiver.REPLY, exchange.id, mutable = true),
                )
                .addRemoteInput(input)
                .setAllowGeneratedReplies(false)
                .setSemanticAction(NotificationCompat.Action.SEMANTIC_ACTION_REPLY)
                .setShowsUserInterface(false)
                .build()
        )
        builder.addAction(plain(R.string.open, open(exchange.id)))
        message.links.firstOrNull()?.let { link ->
            val view = Intent(context, LinkActivity::class.java).setData(resolve(link.href).toUri())
            builder.addAction(plain(R.string.open_link, activity(view, exchange.id)))
        }
    }

    private fun plain(label: Int, intent: PendingIntent) =
        NotificationCompat.Action.Builder(icon, context.getString(label), intent)
            .setAuthenticationRequired(!preferences.plainActionsWhileLocked)
            .build()

    private val icon = IconCompat.createWithResource(context, R.drawable.ic_notification)

    /**
     * Says who answered first, and what, as a message from them after the question. A reply has a
     * choice, text, or both, so there is always something to say.
     */
    private fun answered(
        style: NotificationCompat.MessagingStyle,
        exchange: Exchange,
        reply: Reply,
    ) {
        val choice =
            reply.choice?.let { key ->
                exchange.message.choices.find { it.key == key }?.label ?: key
            }
        val said = listOfNotNull(choice, reply.text).joinToString(" · ")
        val text = context.getString(R.string.answered_first, said)
        style.addMessage(text, reply.at.toEpochMilli(), person(reply.by))
    }

    /**
     * Keeps a long-lived shortcut for [sender], which Android needs to treat the notification as a
     * conversation. It is pushed once each time the app's process starts, since pushing it again
     * changes nothing.
     */
    private fun shortcut(sender: Person) {
        val key = checkNotNull(sender.key)
        if (!shortcuts.add(key)) return
        val shortcut =
            ShortcutInfoCompat.Builder(context, key)
                .setShortLabel(key)
                .setLongLived(true)
                .setPerson(sender)
                .setIcon(IconCompat.createWithResource(context, R.mipmap.ic_launcher))
                .setIntent(Intent(Intent.ACTION_MAIN, null, context, MainActivity::class.java))
                .build()
        ShortcutManagerCompat.pushDynamicShortcut(context, shortcut)
    }

    /** Opens exchange [id] in the app, or the inbox when it is `null`. */
    private fun open(id: String?): PendingIntent =
        activity(
            Intent(context, MainActivity::class.java).apply {
                id?.let { putExtra(OPEN_EXCHANGE, it) }
            },
            id ?: "inbox",
        )

    /**
     * [intent] as a pending activity. Its identifier is [id], so each exchange's pending intents
     * stay apart rather than replacing each other's extras.
     */
    private fun activity(intent: Intent, id: String): PendingIntent =
        PendingIntent.getActivity(
            context,
            0,
            intent.setIdentifier("${intent.action}:$id"),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

    /**
     * [action] on exchange [id], sent to [NotificationReceiver]. A reply's must be [mutable], since
     * Android adds what was typed or chosen to it.
     */
    private fun broadcast(action: String, id: String, mutable: Boolean = false): PendingIntent {
        val intent =
            Intent(context, NotificationReceiver::class.java)
                .setAction(action)
                .setIdentifier(id)
                .putExtra(NotificationReceiver.EXCHANGE, id)
        val mutability = if (mutable) PendingIntent.FLAG_MUTABLE else PendingIntent.FLAG_IMMUTABLE
        return PendingIntent.getBroadcast(
            context,
            0,
            intent,
            mutability or PendingIntent.FLAG_UPDATE_CURRENT,
        )
    }
}

/**
 * The sender or replier [name], a name with its `@`, as Android shows a person in a conversation.
 * No one is marked a bot: transom treats people and agents the same, and Android offers the
 * conversation's choices only for a person.
 */
internal fun person(name: String): Person =
    Person.Builder().setName(name).setKey(name).setBot(false).build()

/** The extra of the intent that opens the app on an exchange, holding its id. */
const val OPEN_EXCHANGE = "sh.transom.extra.EXCHANGE"

private const val ME = "me"
