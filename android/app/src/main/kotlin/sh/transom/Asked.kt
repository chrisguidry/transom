package sh.transom

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import java.io.IOException
import sh.transom.sync.Refused
import sh.transom.theme.BodyLineHeight

/**
 * How a request a person made from a screen turned out, such as a rename or an invitation: the
 * server's [Answered] value, or why there is none. These requests are sent at once rather than
 * queued, since the person is waiting to see whether they worked.
 */
sealed interface Asked<out T> {
    data class Answered<T>(val value: T) : Asked<T>

    data object Unreachable : Asked<Nothing>

    /** The server refused, for the [reason] it gave, such as `the name bob is taken`. */
    data class Refused(val reason: String) : Asked<Nothing>
}

/** Sends [request] and returns how it turned out. */
suspend fun <T> ask(request: suspend () -> T): Asked<T> =
    try {
        Asked.Answered(request())
    } catch (_: IOException) {
        Asked.Unreachable
    } catch (refused: Refused) {
        Asked.Refused(refused.reason)
    }

/**
 * What went wrong with [asked], in a muted line under what the person did, or nothing when it
 * worked. A refusal shows the server's own reason as a sentence, since the server says plainly what
 * it refused and why.
 */
@Composable
fun AskedProblem(asked: Asked<*>?, modifier: Modifier = Modifier) {
    val problem =
        when (asked) {
            Asked.Unreachable -> stringResource(R.string.unreachable)
            is Asked.Refused -> asked.reason.replaceFirstChar { it.uppercase() }.trimEnd('.') + "."
            else -> return
        }
    Text(
        problem,
        modifier,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        lineHeight = BodyLineHeight,
    )
}
