package sh.transom.notifications

import android.app.Notification
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.os.Looper
import androidx.core.app.NotificationCompat
import androidx.core.app.RemoteInput
import kotlinx.coroutines.runBlocking
import org.junit.rules.ExternalResource
import org.robolectric.Robolectric
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ServiceController
import sh.transom.signin.Session
import sh.transom.signin.app
import sh.transom.testserver.TestServer

/**
 * The background connection for a test, signed in as one person, as it runs with the app closed.
 * After the test, it signs the app out, which ends the connection, so no feed outlives its server.
 */
class Connected : ExternalResource() {
    private var service: ServiceController<ConnectionService>? = null

    /**
     * Starts the connection signed in as [name] on [server], and waits until it has begun
     * notifying, so what is sent from here on is news.
     */
    fun signedIn(server: TestServer, name: String): ConnectionService {
        app.sessions.keep(Session(server.url, server.tokenFor(name), name))
        val started = start()
        // Room reads a missing row as 0, so notifying has begun once the time it began is kept.
        await { runBlocking { app.database.notified().kept() } > 0 }
        return started
    }

    /** Starts the connection as Android does, once the app has asked it to start. */
    fun start(): ConnectionService {
        val started =
            Robolectric.buildService(ConnectionService::class.java).create().startCommand(0, 1)
        service = started
        return started.get()
    }

    /** Asks the running connection to start again, as Android does when the app asks again. */
    fun startAgain() {
        checkNotNull(service).startCommand(0, 2)
    }

    override fun after() {
        runBlocking { app.signedOut() }
        service?.destroy()
    }
}

/**
 * Waits up to ten seconds for [condition], running what the main thread has queued before each
 * look, as a notification posted or a broadcast sent is.
 */
fun await(condition: () -> Boolean) {
    val deadline = System.nanoTime() + 10_000_000_000
    while (true) {
        shadowOf(Looper.getMainLooper()).idle()
        if (condition()) return
        check(System.nanoTime() < deadline) { "the condition was not met in ten seconds" }
        Thread.sleep(20)
    }
}

private val manager: NotificationManager
    get() = app.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

/** The notification of exchange [id], or `null` while none is posted. */
fun posted(id: String): Notification? = shadowOf(manager).getNotification(id, Notifier.ID)

/** Exchange [id]'s notification, once one is posted. */
fun awaitPosted(id: String): Notification {
    await { posted(id) != null }
    return posted(id)!!
}

/** Exchange [id]'s notification, once it passes [condition]. */
fun awaitPosted(id: String, condition: (Notification) -> Boolean): Notification {
    await { posted(id)?.let(condition) == true }
    return posted(id)!!
}

/** Waits for exchange [id]'s notification to be cancelled. */
fun awaitCleared(id: String) = await { posted(id) == null }

/** Each message the notification shows, as `sender: text`, the sender `Me` for this person. */
val Notification.messages: List<String>
    get() =
        NotificationCompat.MessagingStyle.extractMessagingStyleFromNotification(this)!!
            .messages
            .map { "${it.person?.name ?: "Me"}: ${it.text}" }

/** The titles of the notification's actions, in order. */
val Notification.actionTitles: List<String>
    get() = actions.orEmpty().map { it.title.toString() }

/** The labels of the chips under the notification's reply box. */
val Notification.chips: List<String>
    get() = actions[0].remoteInputs[0].choices.orEmpty().map { it.toString() }

/**
 * Whether Android posts this notification without a sound. A notification made silent is posted to
 * alert only through its group's summary, which never alerts.
 */
val Notification.silent: Boolean
    get() = groupAlertBehavior == Notification.GROUP_ALERT_SUMMARY

/** Answers from the notification's reply box with [said], tapping a chip when [chip]. */
fun Notification.reply(said: String, chip: Boolean = false) {
    val action = actions[0]
    val results = Intent()
    RemoteInput.addResultsToIntent(
        action.remoteInputs.map { RemoteInput.Builder(it.resultKey).build() }.toTypedArray(),
        results,
        Bundle().apply { putCharSequence(NotificationReceiver.ANSWER, said) },
    )
    RemoteInput.setResultsSource(
        results,
        if (chip) RemoteInput.SOURCE_CHOICE else RemoteInput.SOURCE_FREE_FORM_INPUT,
    )
    action.actionIntent.send(app, 0, results)
    shadowOf(Looper.getMainLooper()).idle()
}

/** Swipes the notification away, as the person does. */
fun Notification.swipe() {
    deleteIntent.send()
    shadowOf(Looper.getMainLooper()).idle()
}
