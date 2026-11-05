package sh.transom.settings

import android.content.Context
import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
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
 * What an urgent alert may do on this phone, under "Urgent alerts": ring through Do Not Disturb,
 * which needs the app's access to Do Not Disturb, and fill the screen over the lock screen, which
 * Android 14 and later ask the person to allow. Each row says where it stands, with Allow while
 * Android has not granted it, the one thing to do on the page. Android grants both in its own
 * settings, so each is read again when the app comes back to the front.
 */
@Composable
internal fun UrgentSettings() {
    val context = LocalContext.current
    val resumed by LocalLifecycleOwner.current.lifecycle.currentStateAsState()
    val doNotDisturb = remember(resumed) { context.mayOverrideDoNotDisturb() }
    val fullScreen = remember(resumed) { context.mayShowFullScreen() }
    Text(
        stringResource(R.string.settings_urgent),
        Modifier.padding(top = 12.dp),
        fontWeight = FontWeight.SemiBold,
    )
    Access(
        R.string.settings_urgent_dnd,
        if (doNotDisturb) R.string.settings_urgent_dnd_on else R.string.settings_urgent_dnd_off,
        doNotDisturb,
        context,
        doNotDisturbAccess(),
    )
    Access(
        R.string.settings_urgent_full_screen,
        if (fullScreen) R.string.settings_urgent_full_screen_on
        else R.string.settings_urgent_full_screen_off,
        fullScreen,
        context,
        context.fullScreenAccess(),
    )
}

/**
 * One grant: its [title], and [state] in words, with a check once [granted], and otherwise Allow,
 * which opens [settings].
 */
@Composable
private fun Access(
    title: Int,
    state: Int,
    granted: Boolean,
    context: Context,
    settings: Intent,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        if (granted) {
            Icon(
                painterResource(R.drawable.ic_check),
                contentDescription = null,
                Modifier.size(20.dp),
                tint = MaterialTheme.colorScheme.primary,
            )
        }
        Column(Modifier.weight(1f)) {
            Text(stringResource(title), lineHeight = BodyLineHeight)
            Note(stringResource(state))
        }
        if (!granted) {
            ActionButton(
                stringResource(R.string.settings_allow),
                { context.startActivity(settings) },
                kind = ButtonKind.Primary,
            )
        }
    }
}
