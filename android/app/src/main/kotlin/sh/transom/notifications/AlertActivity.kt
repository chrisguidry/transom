package sh.transom.notifications

import android.app.KeyguardManager
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.core.net.toUri
import sh.transom.MainActivity
import sh.transom.TransomApplication

/**
 * An urgent exchange full screen, over the lock screen, as an alarm clock shows itself: the
 * notification's full-screen intent starts it when the phone is locked or its screen is off, and
 * the manifest lets it show while locked and turn the screen on.
 *
 * Got it reads the exchange, as swiping its notification away does, so it works without unlocking.
 * A choice answers and a link leaves the lock screen, so each asks the person to unlock first, as
 * the notification's own chips and actions do. The page closes once the exchange is no longer
 * waiting on the person, wherever that happened.
 */
class AlertActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val id = intent.getStringExtra(EXCHANGE)
        val account = (application as TransomApplication).account()
        if (id == null || account == null) {
            finish()
            return
        }
        setContent {
            AlertScreen(
                account,
                id,
                onAcknowledge = { account.background { account.outbox.mark(id, read = true) } },
                onChoose = { key ->
                    unlocked { account.background { account.outbox.reply(id, choice = key) } }
                },
                // LinkActivity hands the link on to whatever opens it, as the notification's own
                // link does.
                onLink = { href ->
                    val link = account.client.resolve(href).toUri()
                    unlocked { startActivity(Intent(this, LinkActivity::class.java).setData(link)) }
                },
                onOpen = {
                    unlocked {
                        startActivity(
                            Intent(this, MainActivity::class.java).putExtra(OPEN_EXCHANGE, id)
                        )
                    }
                },
                onDone = ::finish,
            )
        }
    }

    /** Runs [action] once the phone is unlocked, asking the person to unlock it when it is not. */
    private fun unlocked(action: () -> Unit) {
        val keyguard = getSystemService(KeyguardManager::class.java)
        if (!keyguard.isKeyguardLocked) {
            action()
            return
        }
        keyguard.requestDismissKeyguard(
            this,
            object : KeyguardManager.KeyguardDismissCallback() {
                override fun onDismissSucceeded() = action()
            },
        )
    }

    companion object {
        /** The extra holding the id of the exchange the page shows. */
        const val EXCHANGE = "sh.transom.extra.ALERT"
    }
}
