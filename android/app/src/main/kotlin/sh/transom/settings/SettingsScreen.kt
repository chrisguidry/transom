package sh.transom.settings

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
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
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import sh.transom.Account
import sh.transom.Asked
import sh.transom.AskedProblem
import sh.transom.Place
import sh.transom.R
import sh.transom.ask
import sh.transom.notifications.ConnectionService
import sh.transom.sync.Wake
import sh.transom.theme.ActionButton
import sh.transom.theme.BodyLineHeight
import sh.transom.theme.ButtonKind
import sh.transom.theme.LineField
import sh.transom.theme.PageHeading
import sh.transom.theme.SectionLabel

/**
 * What Settings does outside the account: whether this phone's [notificationsOn], opening Android's
 * notification settings for the app, taking a new name the server gave, and signing out.
 */
class SettingsActions(
    val notificationsOn: Boolean,
    val onNotificationSettings: () -> Unit,
    val onRenamed: (String) -> Unit,
    val onSignOut: () -> Unit,
)

/**
 * Settings, which only the app has: the server, the account and its name, the notifications,
 * answering from the lock screen, the background connection, this identity's devices, signing out,
 * and the licenses. [onOpen] opens the devices or the licenses.
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
            var whileLocked by remember { mutableStateOf(preferences.plainActionsWhileLocked) }
            Toggle(stringResource(R.string.settings_lock_screen), whileLocked) {
                preferences.plainActionsWhileLocked = it
                whileLocked = it
            }
            Note(stringResource(R.string.settings_lock_screen_note))
            // A phone that FCM wakes holds no connection, so it has none to turn off.
            val wakes by account.wakes.collectAsState()
            if (wakes is Wake.Fcm) {
                Text(
                    stringResource(R.string.settings_wakes_by_fcm),
                    Modifier.padding(top = 8.dp),
                    lineHeight = BodyLineHeight,
                )
            } else {
                BackgroundConnection(preferences)
            }
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

/** The background connection's switch, which starts or stops it as it turns on or off. */
@Composable
private fun BackgroundConnection(preferences: Preferences) {
    val context = LocalContext.current
    var connection by remember { mutableStateOf(preferences.backgroundConnection) }
    Toggle(stringResource(R.string.settings_connection), connection) {
        preferences.backgroundConnection = it
        connection = it
        if (it) ConnectionService.start(context) else ConnectionService.stop(context)
    }
    Note(
        stringResource(
            if (connection) R.string.settings_connection_on else R.string.settings_connection_off
        )
    )
}

/** A setting that is on or off: the whole row toggles it, and the switch shows which. */
@Composable
private fun Toggle(text: String, on: Boolean, onChange: (Boolean) -> Unit) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier.fillMaxWidth()
            .toggleable(value = on, role = Role.Switch, onValueChange = onChange)
            .padding(top = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text, Modifier.weight(1f), lineHeight = BodyLineHeight)
        Switch(
            checked = on,
            // The row toggles it, so the switch itself only shows the state.
            onCheckedChange = null,
            colors =
                SwitchDefaults.colors(
                    checkedTrackColor = colors.primary,
                    checkedThumbColor = colors.onPrimary,
                    uncheckedTrackColor = colors.surface,
                    uncheckedBorderColor = colors.outline,
                    uncheckedThumbColor = colors.onSurfaceVariant,
                ),
        )
    }
}

/** A muted line under a setting that says what it does. */
@Composable
private fun Note(text: String) {
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
