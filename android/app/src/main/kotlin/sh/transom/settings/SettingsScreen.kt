package sh.transom.settings

import android.text.format.DateUtils
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.launch
import sh.transom.Account
import sh.transom.Asked
import sh.transom.AskedProblem
import sh.transom.Place
import sh.transom.R
import sh.transom.ask
import sh.transom.delivery.Delivery
import sh.transom.delivery.GooglePlay
import sh.transom.theme.ActionButton
import sh.transom.theme.BodyLineHeight
import sh.transom.theme.ButtonKind
import sh.transom.theme.LineField
import sh.transom.theme.PageHeading
import sh.transom.theme.SectionLabel

/**
 * What Settings does outside the account: whether this phone's [notificationsOn], opening Android's
 * notification settings for the app, taking a new name the server gave, signing out, whether
 * [googlePlay] can deliver here, the [delivery] the phone uses, and switching it with [onDelivery].
 */
class SettingsActions(
    val notificationsOn: Boolean,
    val onNotificationSettings: () -> Unit,
    val onRenamed: (String) -> Unit,
    val onSignOut: () -> Unit,
    val googlePlay: GooglePlay,
    val delivery: Delivery,
    val onDelivery: (Delivery) -> Unit,
)

/**
 * Settings, which only the app has: the server, the account and its name, the notifications,
 * answering from the lock screen, how notifications reach this phone, this identity's devices,
 * signing out, and the licenses. [onOpen] opens the devices or the licenses.
 */
@Composable
fun SettingsScreen(
    account: Account,
    actions: SettingsActions,
    onOpen: (Place) -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val preferences = remember { Preferences(context) }
    Column(
        modifier.verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 24.dp)
    ) {
        PageHeading(stringResource(R.string.settings), Modifier.padding(top = 4.dp))
        Section(R.string.settings_server) { Text(account.session.server) }
        Section(R.string.settings_account) { Rename(account, actions.onRenamed) }
        Section(R.string.settings_notifications) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(
                    stringResource(
                        if (actions.notificationsOn) R.string.settings_notifications_on
                        else R.string.settings_notifications_off
                    ),
                    Modifier.weight(1f),
                    lineHeight = BodyLineHeight,
                )
                ActionButton(
                    stringResource(R.string.settings_notification_settings),
                    actions.onNotificationSettings,
                    kind = ButtonKind.Quiet,
                )
            }
            Note(stringResource(R.string.settings_channels))
            UrgentSettings()
        }
        Section(R.string.settings_delivery) {
            DeliveryChoice(account, actions.googlePlay, actions.delivery, actions.onDelivery)
            Heard(account, preferences)
        }
        // Signing out forgets this phone as one of the devices, so it is among them.
        Section(R.string.devices) {
            Opens(stringResource(R.string.settings_devices)) { onOpen(Place.Devices) }
            Row {
                ActionButton(
                    stringResource(R.string.sign_out),
                    actions.onSignOut,
                    kind = ButtonKind.Quiet,
                )
            }
        }
        Section(R.string.licenses) {
            Opens(stringResource(R.string.settings_licenses)) { onOpen(Place.Licenses) }
        }
    }
}

/** A section as the web draws one: its [label] as an `h2`, then its [content]. */
@Composable
private fun Section(label: Int, content: @Composable () -> Unit) {
    SectionLabel(stringResource(label), Modifier.padding(top = 32.dp, bottom = 8.dp))
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) { content() }
}

/**
 * The account's `@name`, and a new name with Rename. The server says when a name is taken or isn't
 * valid, and that is shown as it says it. Once the server takes the name, [onRenamed] has it.
 */
@Composable
private fun Rename(account: Account, onRenamed: (String) -> Unit) {
    var typed by rememberSaveable { mutableStateOf("") }
    var asked by remember { mutableStateOf<Asked<String>?>(null) }
    val scope = rememberCoroutineScope()
    val label = stringResource(R.string.settings_rename)
    val rename = {
        val name = typed.trim().removePrefix("@")
        if (name.isNotEmpty()) {
            scope.launch {
                val answer = ask { account.client.rename(account.name, name) }
                asked = answer
                (answer as? Asked.Answered)?.let {
                    typed = ""
                    onRenamed(it.value)
                }
            }
        }
    }
    Text(stringResource(R.string.who, account.name), lineHeight = BodyLineHeight)
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        LineField(
            typed,
            { typed = it },
            label,
            stringResource(R.string.settings_new_name),
            onGo = rename,
            Modifier.weight(1f),
        )
        ActionButton(label, rename, kind = ButtonKind.Primary)
    }
    AskedProblem(asked)
}

/**
 * When this phone last heard from its server, so the person can trust that messages arrive: "now"
 * while the live feed is connected, and otherwise how long ago the last event or wake came.
 */
@Composable
private fun Heard(account: Account, preferences: Preferences) {
    val connected by account.sync.connected.collectAsStateWithLifecycle()
    val heard = preferences.heard
    val text =
        when {
            connected -> stringResource(R.string.settings_heard_now)
            heard == null -> stringResource(R.string.settings_heard_never)
            else ->
                stringResource(
                    R.string.settings_heard,
                    DateUtils.getRelativeTimeSpanString(
                        heard.toEpochMilli(),
                        System.currentTimeMillis(),
                        DateUtils.MINUTE_IN_MILLIS,
                    ),
                )
        }
    Note(text)
}

/** A muted line under a setting that says what it does. */
@Composable
internal fun Note(text: String) {
    Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant, lineHeight = BodyLineHeight)
}

/** [text] in the accent, as the web's links are, which opens another screen with [onClick]. */
@Composable
private fun Opens(text: String, onClick: () -> Unit) {
    Text(
        text,
        Modifier.clickable(onClick = onClick).padding(vertical = 4.dp),
        color = MaterialTheme.colorScheme.primary,
        lineHeight = BodyLineHeight,
    )
}
