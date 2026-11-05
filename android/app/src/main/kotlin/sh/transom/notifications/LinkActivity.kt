package sh.transom.notifications

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.os.Bundle

/**
 * Opens a notification's link in the app that handles it, usually the browser. The notification's
 * Open link action names this activity, so its pending intent goes to transom alone: no other app
 * can receive it or change where it leads. The activity is not exported, so only transom's own
 * pending intents start it, and it shows nothing: it hands the link on and finishes.
 */
class LinkActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        intent.data?.let { link ->
            // A phone with nothing that opens the link leaves the tap without an answer, as
            // Android does for a link no app can open.
            try {
                startActivity(Intent(Intent.ACTION_VIEW, link))
            } catch (_: ActivityNotFoundException) {}
        }
        finish()
    }
}
