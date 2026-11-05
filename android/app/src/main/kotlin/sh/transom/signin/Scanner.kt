package sh.transom.signin

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import sh.transom.R
import sh.transom.theme.ActionButton

/**
 * A camera that shows what it sees and reads QR codes from it. [PhoneCamera] is the phone's, and a
 * test gives one that shows a code of its own.
 */
fun interface Camera {
    /** Shows the camera's preview in [modifier], and calls [onText] with each code it reads. */
    @Composable fun Viewfinder(onText: (String) -> Unit, modifier: Modifier)
}

/**
 * The camera's screen: its preview, a line saying what to point it at, and Cancel. A code that
 * holds a server's URL calls [onServer] with it; any other code says so on the line, and the camera
 * keeps looking. Cancel and the back button call [onCancel].
 */
@Composable
internal fun ScanScreen(
    camera: Camera,
    onServer: (String) -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var wrong by remember { mutableStateOf(false) }
    BackHandler(onBack = onCancel)
    Column(
        modifier.padding(20.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        camera.Viewfinder(
            onText = { text ->
                val server = scannedServer(text)
                if (server != null) onServer(server) else wrong = true
            },
            // The preview fills the height the line and Cancel leave, whichever way the phone is
            // turned, rather than a fixed shape that a turned phone has no room for.
            modifier = Modifier.fillMaxWidth().weight(1f).clip(RoundedCornerShape(4.dp)),
        )
        Text(
            text = stringResource(if (wrong) R.string.scan_not_a_server else R.string.scan_point),
            textAlign = TextAlign.Center,
            lineHeight = 1.55.em,
        )
        ActionButton(stringResource(R.string.scan_cancel), onClick = onCancel)
    }
}
