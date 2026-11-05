package sh.transom.signin

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts.RequestPermission
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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
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
import java.io.IOException
import java.net.SocketTimeoutException
import java.net.UnknownHostException
import java.security.cert.CertificateException
import javax.net.ssl.SSLHandshakeException
import javax.net.ssl.SSLPeerUnverifiedException
import sh.transom.R

/**
 * The sign-in screen, as the web's: a centered "Sign in", then the server's URL and Continue, which
 * opens the server's own sign-in page. [problem] says what went wrong with the last try, and
 * [working] holds Continue while a try is under way.
 *
 * When a server on the local network can't be reached on Android 17 or later, the screen asks for
 * the local network permission, and tries again once it is granted.
 */
@Composable
fun SignInScreen(
    problem: Problem?,
    working: Boolean,
    onContinue: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var typed by rememberSaveable { mutableStateOf("") }
    var tries by rememberSaveable { mutableIntStateOf(0) }
    val go = {
        if (!working) {
            tries++
            onContinue(typed)
        }
    }
    val refused = askForLocalNetwork(problem, tries, retry = { onContinue(typed) })
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
                text =
                    stringResource(if (refused) R.string.sign_in_local_network else it.message()),
                textAlign = TextAlign.Center,
                lineHeight = 1.55.em,
            )
        }
        // The cause under the message lets a screenshot of a failure say what failed.
        (problem as? Problem.Unreachable)?.let {
            Text(
                text =
                    classified(it.cause)?.let { reason -> stringResource(reason) }
                        ?: described(it.cause),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                fontSize = 14.sp,
                textAlign = TextAlign.Center,
            )
        }
    }
}

/**
 * Asks for the local network permission when [problem] is a server there that Android may have kept
 * the app from reaching, and calls [retry] once it is granted. It asks at most once for each of the
 * person's [tries], so a retry that fails again shows its problem rather than asking again. Returns
 * whether the person refused.
 */
@Composable
private fun askForLocalNetwork(problem: Problem?, tries: Int, retry: () -> Unit): Boolean {
    val context = LocalContext.current
    // Both are saved with the activity, so a phone turned while the dialog is up neither asks again
    // nor forgets the answer.
    var asked by rememberSaveable(tries) { mutableStateOf(false) }
    var refused by rememberSaveable(problem) { mutableStateOf(false) }
    val ask =
        rememberLauncherForActivityResult(RequestPermission()) { granted ->
            if (granted) retry() else refused = true
        }
    LaunchedEffect(problem) {
        if (!asked && context.asksForLocalNetwork(problem)) {
            asked = true
            ask.launch(LOCAL_NETWORK)
        }
    }
    return refused
}

private fun Problem.message(): Int =
    when (this) {
        Problem.NotAnAddress -> R.string.sign_in_not_an_address
        is Problem.Unreachable -> R.string.sign_in_unreachable
        Problem.NotTransom -> R.string.sign_in_not_transom
        Problem.Cancelled -> R.string.sign_in_cancelled
        Problem.Refused -> R.string.sign_in_refused
        Problem.NoBrowser -> R.string.sign_in_no_browser
    }

/** Why a server can't be reached, as a few plain words, for the failures people meet most. */
internal fun classified(cause: IOException): Int? =
    when {
        cause is UnknownHostException -> R.string.sign_in_no_address
        cause is SocketTimeoutException -> R.string.sign_in_timed_out
        cause is SSLHandshakeException && cause.cause is CertificateException ->
            R.string.sign_in_untrusted
        cause is SSLPeerUnverifiedException -> R.string.sign_in_another_name
        else -> null
    }

/**
 * The messages of [cause] and of what caused it, for a failure [classified] has no words for.
 * OkHttp's own message names the address it tried, and the system's under it says why, such as
 * `ECONNREFUSED`.
 */
internal fun described(cause: IOException): String =
    generateSequence<Throwable>(cause) { it.cause }
        .mapNotNull { it.message }
        .distinct()
        .joinToString(": ")
        .ifEmpty { cause.javaClass.simpleName }
