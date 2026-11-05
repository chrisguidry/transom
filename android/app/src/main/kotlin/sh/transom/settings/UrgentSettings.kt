package sh.transom.settings

import android.content.Context
import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.currentStateAsState
import sh.transom.R
import sh.transom.notifications.doNotDisturbAccess
import sh.transom.notifications.fullScreenAccess
import sh.transom.notifications.mayOverrideDoNotDisturb
import sh.transom.notifications.mayShowFullScreen
import sh.transom.theme.ActionButton
import sh.transom.theme.BodyLineHeight
import sh.transom.theme.ButtonKind

/**
 * What an urgent message may do on this phone, each with Allow while Android has not granted it:
 * ring through Do Not Disturb, which needs the app's access to Do Not Disturb, and show full screen
 * over the lock screen, which Android 14 and later ask the person to allow. Android grants both in
 * its own settings, so each is read again when the app comes back to the front.
 */
@Composable
internal fun UrgentSettings() {
    val context = LocalContext.current
    val resumed by LocalLifecycleOwner.current.lifecycle.currentStateAsState()
    val doNotDisturb = remember(resumed) { context.mayOverrideDoNotDisturb() }
    val fullScreen = remember(resumed) { context.mayShowFullScreen() }
    Access(
        doNotDisturb,
        R.string.settings_urgent_dnd_on,
        R.string.settings_urgent_dnd_off,
        context,
        doNotDisturbAccess(),
    )
    Access(
        fullScreen,
        R.string.settings_urgent_full_screen_on,
        R.string.settings_urgent_full_screen_off,
        context,
        context.fullScreenAccess(),
    )
}

/** What [granted] means, in [on] or [off], with Allow, which opens [settings], while it is off. */
@Composable
private fun Access(granted: Boolean, on: Int, off: Int, context: Context, settings: Intent) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(
            stringResource(if (granted) on else off),
            Modifier.weight(1f),
            lineHeight = BodyLineHeight,
        )
        if (!granted) {
            ActionButton(
                stringResource(R.string.settings_allow),
                { context.startActivity(settings) },
                kind = ButtonKind.Quiet,
            )
        }
    }
}
