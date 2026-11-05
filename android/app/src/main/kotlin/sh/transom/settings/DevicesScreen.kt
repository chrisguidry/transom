package sh.transom.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import sh.transom.Account
import sh.transom.Asked
import sh.transom.AskedProblem
import sh.transom.R
import sh.transom.ask
import sh.transom.theme.ActionButton
import sh.transom.theme.BodyLineHeight
import sh.transom.theme.ButtonKind
import sh.transom.theme.MetaText
import sh.transom.theme.PageHeading

/**
 * [account]'s devices: each one's name over how it wakes, with this phone marked. Forget removes
 * any other device, and forgetting a phone signs it out. Forgetting this phone would be signing
 * out, so this phone offers [onSignOut] instead.
 */
@Composable
fun DevicesScreen(account: Account, onSignOut: () -> Unit, modifier: Modifier = Modifier) {
    val client = account.client
    var listings by remember { mutableIntStateOf(0) }
    var listed by remember { mutableStateOf<Asked<List<Device>>?>(null) }
    var thisPhone by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(listings) {
        thisPhone = account.deviceId()
        listed = ask { client.devices(account.name) }
    }
    var forgot by remember { mutableStateOf<Asked<Unit>?>(null) }
    val scope = rememberCoroutineScope()
    val forget: (Device) -> Unit = { device ->
        scope.launch {
            forgot = ask { client.forget(account.name, device.id) }
            listings++
        }
    }
    Column(
        modifier
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp, vertical = 24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        PageHeading(stringResource(R.string.devices), Modifier.padding(top = 4.dp))
        AskedProblem(listed)
        Column {
            // This phone first, then the others in the server's order.
            val devices = (listed as? Asked.Answered)?.value.orEmpty()
            devices
                .sortedByDescending { it.id == thisPhone }
                .forEach { device ->
                    val mine = device.id == thisPhone
                    DeviceRow(device, mine, onSignOut, onForget = { forget(device) })
                }
        }
        AskedProblem(forgot)
        Text(
            stringResource(R.string.devices_note),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            lineHeight = BodyLineHeight,
        )
    }
}

/**
 * One device, ruled below: its name over how it wakes, and Forget, or Sign out when it is [mine],
 * this phone.
 */
@Composable
private fun DeviceRow(device: Device, mine: Boolean, onSignOut: () -> Unit, onForget: () -> Unit) {
    val wakes = stringResource(device.wakes.label)
    val meta = if (mine) stringResource(R.string.devices_this_phone, wakes) else wakes
    Column {
        Row(
            Modifier.padding(vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Column(Modifier.weight(1f)) {
                Text(device.name.ifEmpty { device.id }, lineHeight = BodyLineHeight)
                MetaText(AnnotatedString(meta))
            }
            ActionButton(
                stringResource(if (mine) R.string.sign_out else R.string.devices_forget),
                if (mine) onSignOut else onForget,
                kind = ButtonKind.Quiet,
            )
        }
        HorizontalDivider(thickness = 1.dp, color = MaterialTheme.colorScheme.outline)
    }
}
