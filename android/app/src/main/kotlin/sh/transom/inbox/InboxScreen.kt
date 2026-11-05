package sh.transom.inbox

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import sh.transom.R

/** The inbox, which has nothing in it until the app signs in and syncs. */
@Composable
fun InboxScreen(modifier: Modifier = Modifier) {
    Column(modifier.padding(horizontal = 20.dp, vertical = 24.dp)) {
        Text(
            text = stringResource(R.string.inbox_heading),
            modifier = Modifier.padding(top = 4.dp, bottom = 16.dp).semantics { heading() },
            fontSize = 24.sp,
            fontWeight = FontWeight.SemiBold,
        )
        Text(
            text = stringResource(R.string.inbox_empty),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            lineHeight = 1.55.em,
        )
    }
}
