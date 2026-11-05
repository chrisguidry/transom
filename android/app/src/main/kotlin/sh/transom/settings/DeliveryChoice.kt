package sh.transom.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.RadioButtonDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import sh.transom.Account
import sh.transom.R
import sh.transom.delivery.Delivery
import sh.transom.delivery.DeliveryState
import sh.transom.delivery.GooglePlay
import sh.transom.theme.BodyLineHeight

/**
 * How notifications reach this phone, one of two: Google Play services, which is offered only where
 * [googlePlay] is available and says why not where it isn't, or the direct connection. [chosen] is
 * the delivery the phone uses now, and [onChoose] switches it. While Google Play services is chosen
 * and fails, the reason shows under the choice.
 */
@Composable
internal fun DeliveryChoice(
    account: Account,
    googlePlay: GooglePlay,
    chosen: Delivery,
    onChoose: (Delivery) -> Unit,
) {
    var selected by remember { mutableStateOf(chosen) }
    val choose = { delivery: Delivery ->
        selected = delivery
        onChoose(delivery)
    }
    Column(Modifier.selectableGroup()) {
        Option(
            stringResource(R.string.settings_delivery_google_play),
            stringResource(R.string.settings_delivery_google_play_note),
            selected == Delivery.GooglePlay,
            enabled = googlePlay is GooglePlay.Available,
            unavailable =
                (googlePlay as? GooglePlay.Unavailable)?.let { stringResource(it.reason) },
        ) {
            choose(Delivery.GooglePlay)
        }
        Option(
            stringResource(R.string.settings_delivery_connection),
            stringResource(R.string.settings_delivery_connection_note),
            selected == Delivery.Connection,
        ) {
            choose(Delivery.Connection)
        }
    }
    val state by account.delivery.collectAsState()
    (state as? DeliveryState.Failing)?.let { Note(stringResource(it.reason)) }
}

/**
 * One delivery to choose: the whole row chooses it, and the radio button shows whether it is
 * [selected]. An option that isn't [enabled] says why it is [unavailable].
 */
@Composable
private fun Option(
    text: String,
    note: String,
    selected: Boolean,
    enabled: Boolean = true,
    unavailable: String? = null,
    onClick: () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier.fillMaxWidth()
            .selectable(selected, enabled, Role.RadioButton, onClick = onClick)
            .padding(vertical = 8.dp),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        RadioButton(
            selected = selected,
            // The row chooses it, so the button itself only shows the choice.
            onClick = null,
            enabled = enabled,
            colors =
                RadioButtonDefaults.colors(
                    selectedColor = colors.primary,
                    unselectedColor = colors.onSurfaceVariant,
                    disabledUnselectedColor = colors.outline,
                ),
        )
        Column(Modifier.weight(1f)) {
            Text(
                text,
                color = if (enabled) colors.onSurface else colors.onSurfaceVariant,
                lineHeight = BodyLineHeight,
            )
            Note(note)
            unavailable?.let { Note(it) }
        }
    }
}
