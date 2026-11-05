package sh.transom.notifications

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.core.app.RemoteInput
import sh.transom.TransomApplication

/**
 * What the person does to an exchange's notification without opening the app: answers from its
 * reply box, with a chip or with text, or swipes it away. Each is done as the signed-in account,
 * and ignored when no one is signed in.
 */
class NotificationReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val id = intent.getStringExtra(EXCHANGE) ?: return
        val account = (context.applicationContext as TransomApplication).account() ?: return
        val notifier = account.notifier
        // The receiver is not exported, so the only intents it gets are the two the notifications
        // carry: a reply, or a dismissal.
        val work: suspend () -> Unit =
            if (intent.action == REPLY) {
                val said =
                    RemoteInput.getResultsFromIntent(intent)
                        ?.getCharSequence(ANSWER)
                        ?.toString()
                        ?.trim()
                        .orEmpty()
                if (said.isEmpty()) return
                val chose = RemoteInput.getResultsSource(intent) == RemoteInput.SOURCE_CHOICE
                ({ notifier.answer(id, said, chose) })
            } else {
                ({ notifier.dismissed(id) })
            }
        // The receiver's process may be stopped as soon as it returns, so it holds the broadcast
        // open until the work is done.
        val pending = goAsync()
        account.background(work).invokeOnCompletion { pending.finish() }
    }

    companion object {
        /** The action of a reply from the notification's reply box. */
        const val REPLY = "sh.transom.action.REPLY"

        /** The action of a notification the person swiped away. */
        const val DISMISS = "sh.transom.action.DISMISS"

        /** The extra holding the exchange's id. */
        const val EXCHANGE = "sh.transom.extra.EXCHANGE"

        /** The reply box's key among the results Android adds to the intent. */
        const val ANSWER = "answer"
    }
}
