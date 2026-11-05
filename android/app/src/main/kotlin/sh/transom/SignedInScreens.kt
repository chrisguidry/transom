package sh.transom

import androidx.activity.compose.BackHandler
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.ui.Modifier
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import sh.transom.exchange.ExchangeScreen
import sh.transom.inbox.InboxScreen

/**
 * The signed-in app: the inbox, or the exchange [open] over it, which the system's back gesture
 * closes. [onOpen] opens an exchange by its id, or the inbox with `null`.
 *
 * The app has two screens and one argument, so the navigator is this state rather than
 * navigation-compose, which would add routes, a back stack, and a library for what one nullable id
 * and a [BackHandler] do. The inbox keeps its scroll position while an exchange is open, since its
 * saved state is held under a key of its own.
 *
 * While the app is on screen, at least started, it follows [account]'s live feed, as the web inbox
 * follows its own, and stops when the app leaves the screen.
 */
@Composable
fun SignedInScreens(
    account: Account,
    open: String?,
    onOpen: (String?) -> Unit,
    modifier: Modifier = Modifier,
) {
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(account, lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) { account.follow() }
    }
    // Read here rather than in the inbox's screen, so the inbox is whole the moment it comes back
    // from an exchange, and its list restores its scroll position into the same items.
    val inbox by
        remember(account) { account.store.inbox(account.name) }.collectAsStateWithLifecycle(null)
    BackHandler(enabled = open != null) { onOpen(null) }
    val saved = rememberSaveableStateHolder()
    if (open == null) {
        saved.SaveableStateProvider(INBOX) {
            InboxScreen(account.name, inbox, { onOpen(it) }, modifier)
        }
    } else {
        saved.SaveableStateProvider(open) { ExchangeScreen(account, open, modifier) }
    }
}

/** The inbox's key among the saved states, which no exchange's id can be. */
private const val INBOX = "inbox"
