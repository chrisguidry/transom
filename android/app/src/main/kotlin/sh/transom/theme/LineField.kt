package sh.transom.theme

import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * The web's `input` for a name: one line, ruled, with the accent's outline while focused, and
 * [placeholder] in the muted color while it is empty. [label] is what a screen reader calls it, and
 * the keyboard's Go key calls [onGo].
 */
@Composable
fun LineField(
    text: String,
    onChange: (String) -> Unit,
    label: String,
    placeholder: String,
    onGo: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val colors = MaterialTheme.colorScheme
    val style = TextStyle(color = colors.onSurface, fontSize = 16.sp)
    BasicTextField(
        value = text,
        onValueChange = onChange,
        modifier =
            modifier
                .semantics { contentDescription = label }
                .border(
                    if (focused) 2.dp else 1.dp,
                    if (focused) colors.primary else colors.outline,
                    RoundedCornerShape(4.dp),
                )
                .padding(horizontal = 10.dp, vertical = 9.dp),
        textStyle = style,
        cursorBrush = SolidColor(colors.primary),
        singleLine = true,
        // A name is lowercase, so the keyboard neither capitalizes nor corrects it.
        keyboardOptions =
            KeyboardOptions(
                capitalization = KeyboardCapitalization.None,
                autoCorrectEnabled = false,
                imeAction = ImeAction.Go,
            ),
        keyboardActions = KeyboardActions(onGo = { onGo() }),
        interactionSource = interaction,
        decorationBox = { field ->
            Box {
                if (text.isEmpty())
                    Text(placeholder, style = style.copy(color = colors.onSurfaceVariant))
                field()
            }
        },
    )
}
