package sh.transom.signin

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import sh.transom.R

/**
 * The sign-in screen, as the web's: a centered "Sign in", then the server's URL and Continue, which
 * opens the server's own sign-in page. [problem] says what went wrong with the last try, and
 * [working] holds Continue while a try is under way.
 */
@Composable
fun SignInScreen(
    problem: Problem?,
    working: Boolean,
    onContinue: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var typed by rememberSaveable { mutableStateOf("") }
    val go = { if (!working) onContinue(typed) }
    Column(
        modifier.padding(start = 20.dp, end = 20.dp, top = 64.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text(
            text = stringResource(R.string.sign_in_heading),
            modifier = Modifier.semantics { heading() },
            fontSize = 24.sp,
            fontWeight = FontWeight.SemiBold,
        )
        OutlinedTextField(
            value = typed,
            onValueChange = { typed = it },
            modifier = Modifier.fillMaxWidth(),
            label = { Text(stringResource(R.string.sign_in_server)) },
            placeholder = { Text(stringResource(R.string.sign_in_server_example)) },
            singleLine = true,
            shape = RoundedCornerShape(4.dp),
            colors =
                OutlinedTextFieldDefaults.colors(
                    unfocusedBorderColor = MaterialTheme.colorScheme.outline
                ),
            keyboardOptions =
                KeyboardOptions(
                    keyboardType = KeyboardType.Uri,
                    autoCorrectEnabled = false,
                    imeAction = ImeAction.Go,
                ),
            keyboardActions = KeyboardActions(onGo = { go() }),
        )
        Button(onClick = go, enabled = !working, shape = RoundedCornerShape(4.dp)) {
            Text(stringResource(R.string.sign_in_continue))
        }
        problem?.let {
            Text(
                text = stringResource(it.message()),
                textAlign = TextAlign.Center,
                lineHeight = 1.55.em,
            )
        }
    }
}

private fun Problem.message(): Int =
    when (this) {
        Problem.NotAnAddress -> R.string.sign_in_not_an_address
        Problem.Unreachable -> R.string.sign_in_unreachable
        Problem.NotTransom -> R.string.sign_in_not_transom
        Problem.Cancelled -> R.string.sign_in_cancelled
        Problem.Refused -> R.string.sign_in_refused
        Problem.NoBrowser -> R.string.sign_in_no_browser
    }
