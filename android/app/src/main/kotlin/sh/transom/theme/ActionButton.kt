package sh.transom.theme

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

/** The web UI's buttons: `button`, `button.primary`, and `button.quiet`. */
enum class ButtonKind {
    /** An accent outline and accent text, for each of a question's choices. */
    Default,
    /** Filled with the accent, for the one thing a form is for. */
    Primary,
    /** A rule's outline and muted text, for what is seldom wanted, such as withdrawing. */
    Quiet,
}

/** A button as the web UI draws one of [kind]: a 1 dp outline with 4 dp corners. */
@Composable
fun ActionButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    kind: ButtonKind = ButtonKind.Default,
    enabled: Boolean = true,
) {
    val colors = MaterialTheme.colorScheme
    val outline = if (kind == ButtonKind.Quiet) colors.outline else colors.primary
    val content =
        when (kind) {
            ButtonKind.Default -> colors.primary
            ButtonKind.Primary -> colors.onPrimary
            ButtonKind.Quiet -> colors.onSurfaceVariant
        }
    val fill = if (kind == ButtonKind.Primary) colors.primary else Color.Transparent
    OutlinedButton(
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        shape = RoundedCornerShape(4.dp),
        colors =
            ButtonDefaults.outlinedButtonColors(
                containerColor = fill,
                contentColor = content,
                disabledContainerColor = Color.Transparent,
                disabledContentColor = colors.onSurfaceVariant,
            ),
        border = BorderStroke(1.dp, if (enabled) outline else colors.outline),
        contentPadding = PaddingValues(horizontal = 14.dp, vertical = 7.dp),
    ) {
        Text(text)
    }
}
