package sh.transom

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp

/**
 * The bar across the top of every screen, as the web UI's `.bar`: the wordmark over a rule, and
 * once signed in, the `@name` and a menu where the web has its Sign out button. The wordmark goes
 * home to the inbox, as the web's links to it.
 */
@Composable
fun TopBar(name: String?, onHome: () -> Unit, onSignOut: () -> Unit) {
    Column {
        Row(
            Modifier.padding(start = 20.dp, end = 8.dp).padding(vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = stringResource(R.string.app_name),
                modifier = Modifier.clickable(onClick = onHome).padding(vertical = 8.dp),
                fontSize = 16.sp,
                fontWeight = FontWeight.SemiBold,
                letterSpacing = 0.02.em,
            )
            Spacer(Modifier.weight(1f))
            name?.let {
                Text(
                    text = stringResource(R.string.who, it),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Menu(onSignOut)
            }
        }
        HorizontalDivider(thickness = 1.dp, color = MaterialTheme.colorScheme.outline)
    }
}

@Composable
private fun Menu(onSignOut: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    // A dropdown opens from the start of its parent, so the box makes the
    // button its parent and the menu opens under it.
    Box {
        IconButton(onClick = { open = true }) {
            Icon(
                painterResource(R.drawable.ic_menu),
                contentDescription = stringResource(R.string.menu),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            DropdownMenuItem(
                text = { Text(stringResource(R.string.sign_out)) },
                onClick = {
                    open = false
                    onSignOut()
                },
            )
        }
    }
}
