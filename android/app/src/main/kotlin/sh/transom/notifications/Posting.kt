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
import java.io.File
import java.time.Instant
import sh.transom.MainActivity
import sh.transom.R
import sh.transom.exchange.Exchange
import sh.transom.exchange.Reply
import sh.transom.exchange.Urgency
import sh.transom.pictures.Pictures
import sh.transom.pictures.avatar
import sh.transom.sync.Client

/** How a notification that is posted alerts the person. */
internal enum class Alert {
    /** It makes no sound and no heads-up. */
    QUIET,

    /** It alerts as its channel says, unless Android shows it already. */
    ONCE,

    /** It alerts as its channel says even though Android shows it already, as a repeat does. */
    AGAIN,
}

/**
 * Makes the notification of each [Shown] exchange. Each sender is a conversation: the notification
 * is `MessagingStyle`, from the sender as a [Person] with the sender's picture, with a long-lived
 * shortcut of the sender's own, so Android's long-press menu lets the person make one sender a
 * priority conversation or silence it.
 *
 * The first image the message carries is a message of its own in the conversation, which Android
 * shows in the expanded notification. It comes before the text: Android keeps a conversation's last
 * messages in view, and a tall image after the text pushes the summary out of sight. The collapsed
 * notification shows the text with the image beside it. `BigPictureStyle` would show it larger, but
 * a notification has one style, and that one is no conversation: the sender's own settings in the
 * long-press menu, and its place in the shade's conversations, would go with it.
 */
internal class Posting(private val context: Context, private val client: Client) {
    private val markdown = notificationMarkdown(client::resolve)
    private val me = Person.Builder().setName(context.getString(R.string.me)).setKey(ME).build()
    private val shortcuts = mutableMapOf<String, String>()
    private val pictures = Pictures(client, File(context.cacheDir, PICTURES))
    val images = NotificationImages(context, client)

    /** The notification [shown] makes, posted to alert as [alert] says, at [now]. */
    suspend fun notification(shown: Shown, alert: Alert, now: Instant): Notification {
        val exchange = shown.entry.exchange
        val urgency = exchange.message.urgency
        val sender = person(exchange.from, exchange.fromName, exchange.fromPicture)
        shortcut(sender, looks = "${sender.name}\n${exchange.fromPicture}")
        val style = NotificationCompat.MessagingStyle(me)
        images.first(exchange)?.let { (type, uri) ->
            style.addMessage(
                NotificationCompat.MessagingStyle.Message(
                        context.getString(R.string.image),
                        exchange.created.toEpochMilli(),
                        sender,
                    )
                    .setData(type, uri)
            )
        }
        style.addMessage(
            notificationText(exchange.message, markdown),
            exchange.created.toEpochMilli(),
            sender,
        )
        when (shown) {
            is Shown.Asking -> {}
            is Shown.Sent -> style.addMessage(shown.said, now.toEpochMilli(), null as Person?)
            is Shown.Answered -> answered(style, exchange, shown.reply)
        }
        val urgent = urgency == Urgency.URGENT
        val builder =
            NotificationCompat.Builder(context, Channels.of(urgency, context))
                .setSmallIcon(R.drawable.ic_notification)
                .setStyle(style)
                .setShortcutId(sender.key)
                .setCategory(
                    if (urgent) NotificationCompat.CATEGORY_ALARM
                    else NotificationCompat.CATEGORY_MESSAGE
                )
                .setWhen(exchange.created.toEpochMilli())
                .setShowWhen(true)
                .setOnlyAlertOnce(alert != Alert.AGAIN)
                .setSilent(alert == Alert.QUIET || shown !is Shown.Asking)
                .setAutoCancel(true)
                .setAllowSystemGeneratedContextualActions(false)
                .setContentIntent(open(exchange.id))
                .setDeleteIntent(broadcast(NotificationReceiver.DISMISS, exchange.id))
        exchange.expires?.let {
            builder.setTimeoutAfter((it.toEpochMilli() - now.toEpochMilli()).coerceAtLeast(1))
        }
        // The full-screen page shows only as the notification alerts, and only where the person
        // allows it; otherwise Android shows the notification as a heads-up, as for high urgency.
        val alerting = urgent && shown is Shown.Asking && alert != Alert.QUIET
        if (alerting && context.mayShowFullScreen()) {
            builder.setFullScreenIntent(page(exchange.id), true)
        }
        // How many times an urgent message has rung, so a person who checks the phone knows
        // whether more rings are coming.
        if (urgent && shown is Shown.Asking) {
            val rang = exchange.repeats + 1
            builder.setSubText(
                context.resources.getQuantityString(R.plurals.rang, RINGS, rang, RINGS)
            )
        }
        if (shown is Shown.Asking) actions(builder, exchange)
        return builder.build()
    }

    /**
     * The notification's buttons, three at most, since Android drops any after the third. Tapping
     * the notification opens the exchange, so no button does.
     * - An urgent message leads with Acknowledge, as its full-screen page does.
     * - A question with a prompt answers in a reply box labelled with the prompt, with the choices
     *   as chips when they all fit. Without a prompt, each choice is a button of its own, or, when
     *   they do not all fit, Answer… opens the exchange, so no choice is dropped without a sign.
     * - Then the links, each labelled with its title. An alert whose choices are actions beside its
     *   links, such as Silence beside a dashboard, leads with its first link.
     *
     * Answering and following a link need an unlocked phone. Acknowledge, which reads the message
     * as swiping it away does, needs none.
     */
    private fun actions(builder: NotificationCompat.Builder, exchange: Exchange) {
        val message = exchange.message
        val id = exchange.id
        val acknowledge =
            listOfNotNull(
                if (message.urgency == Urgency.URGENT)
                    action(
                        context.getString(R.string.acknowledge),
                        broadcast(NotificationReceiver.ACKNOWLEDGE, id),
                        unlocked = false,
                    )
                else null
            )
        val links =
            message.links.map { link ->
                val view =
                    Intent(context, LinkActivity::class.java)
                        .setData(client.resolve(link.href).toUri())
                action(
                    link.title ?: context.getString(R.string.open_link),
                    activity(view, "$id:${link.href}"),
                )
            }
        val leading =
            if (message.prompt == null && message.choices.isNotEmpty()) links.take(1)
            else emptyList()
        val room = MOST_ACTIONS - acknowledge.size - leading.size
        val answers =
            when {
                message.prompt != null -> listOf(reply(exchange))
                message.choices.size > room ->
                    listOf(action(context.getString(R.string.answer_in_app), open(id)))
                else ->
                    message.choices.map {
                        action(
                            it.label,
                            broadcast(NotificationReceiver.CHOOSE, id, choice = it.key),
                        )
                    }
            }
        (acknowledge + leading + answers + (links - leading.toSet()))
            .take(MOST_ACTIONS)
            .forEach(builder::addAction)
    }

    /**
     * The reply box, labelled with the prompt, which takes text, and offers the choices as chips
     * when they all fit.
     */
    private fun reply(exchange: Exchange): NotificationCompat.Action {
        val message = exchange.message
        val label = message.prompt?.takeIf { it.isNotEmpty() } ?: context.getString(R.string.reply)
        val input =
            RemoteInput.Builder(NotificationReceiver.ANSWER)
                .setLabel(label)
                .setChoices(message.chips.map { it.label }.toTypedArray())
                .setAllowFreeFormInput(true)
                .build()
        return NotificationCompat.Action.Builder(
                icon,
                context.getString(R.string.reply),
                broadcast(NotificationReceiver.REPLY, exchange.id, mutable = true),
            )
            .addRemoteInput(input)
            .setAllowGeneratedReplies(false)
            .setSemanticAction(NotificationCompat.Action.SEMANTIC_ACTION_REPLY)
            .setShowsUserInterface(false)
            .build()
    }

    private fun action(label: String, intent: PendingIntent, unlocked: Boolean = true) =
        NotificationCompat.Action.Builder(icon, label, intent)
            .setAuthenticationRequired(unlocked)
            .build()

    private val icon = IconCompat.createWithResource(context, R.drawable.ic_notification)

    /**
     * Says who answered first, and what, as a message from them after the question. A reply has a
     * choice, text, or both, so there is always something to say.
     */
    private suspend fun answered(
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
        style.addMessage(
            text,
            reply.at.toEpochMilli(),
            person(reply.by, reply.byName, reply.byPicture),
        )
    }

    /**
     * The party [handle], a name with its `@`, as Android shows a person in a conversation: by its
     * [name] when it has one, with its [picture] or its initials. The picture carries no urgency:
     * the shade's bell, the channel, and an urgent message's alarm already say it. No one is marked
     * a bot: transom treats people and agents the same, and Android offers the conversation's
     * choices only for a person.
     */
    private suspend fun person(handle: String, name: String?, picture: String?): Person {
        val shown = name ?: handle
        val drawn = avatar(pictures.bitmap(picture, ICON), shown, ICON)
        return Person.Builder()
            .setName(shown)
            .setKey(handle)
            .setBot(false)
            .setIcon(IconCompat.createWithBitmap(drawn))
            .build()
    }

    /**
     * Keeps a long-lived shortcut for [sender], which Android needs to treat the notification as a
     * conversation, with the sender's name and icon. It is pushed again only when how it [looks]
     * changes, as for a message of another urgency, since pushing it again changes nothing.
     */
    private fun shortcut(sender: Person, looks: String) {
        val key = checkNotNull(sender.key)
        if (shortcuts.put(key, looks) == looks) return
        val shortcut =
            ShortcutInfoCompat.Builder(context, key)
                .setShortLabel(checkNotNull(sender.name))
                .setLongLived(true)
                .setPerson(sender)
                .setIcon(sender.icon)
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

    /** Shows urgent exchange [id] full screen, over the lock screen. */
    private fun page(id: String): PendingIntent =
        activity(
            Intent(context, AlertActivity::class.java)
                .putExtra(AlertActivity.EXCHANGE, id)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            id,
        )

    /**
     * [intent] as a pending activity. Its identifier is [id], so each exchange's pending intents
     * stay apart rather than replacing each other's extras.
     */
    private fun activity(intent: Intent, id: String): PendingIntent =
        PendingIntent.getActivity(
            context,
            0,
            intent.setIdentifier("${intent.component?.className}:${intent.action}:$id"),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

    /**
     * [action] on exchange [id], sent to [NotificationReceiver]. A reply's must be [mutable], since
     * Android adds what was typed or chosen to it.
     */
    private fun broadcast(
        action: String,
        id: String,
        mutable: Boolean = false,
        choice: String? = null,
    ): PendingIntent {
        // A choice's button carries its key, so each choice's intent is told apart by it, or
        // Android would give every button the extras of the last.
        val intent =
            Intent(context, NotificationReceiver::class.java)
                .setAction(action)
                .setIdentifier(listOfNotNull(id, choice).joinToString(":"))
                .putExtra(NotificationReceiver.EXCHANGE, id)
                .putExtra(NotificationReceiver.CHOICE, choice)
        val mutability = if (mutable) PendingIntent.FLAG_MUTABLE else PendingIntent.FLAG_IMMUTABLE
        return PendingIntent.getBroadcast(
            context,
            0,
            intent,
            mutability or PendingIntent.FLAG_UPDATE_CURRENT,
        )
    }
}

/** The extra of the intent that opens the app on an exchange, holding its id. */
const val OPEN_EXCHANGE = "sh.transom.extra.EXCHANGE"

/** The directory under the app's cache where the pictures of identities are kept. */
internal const val PICTURES = "pictures"

/**
 * How many pixels a side a sender's icon is drawn at, enough for the largest icon Android shows.
 */
private const val ICON = 192

/** The most buttons Android shows under a notification; it drops any after them. */
private const val MOST_ACTIONS = 3

/** How many times an urgent message rings: once, and three repeats, as plan 18 schedules them. */
internal const val RINGS = 4

private const val ME = "me"
