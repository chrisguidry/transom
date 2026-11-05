package sh.transom

import androidx.activity.compose.BackHandler
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.ui.Modifier
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.launch
import sh.transom.exchange.ExchangeScreen
import sh.transom.groups.MembersScreen
import sh.transom.inbox.InboxActions
import sh.transom.inbox.InboxScreen
import sh.transom.settings.DevicesScreen
import sh.transom.settings.LicensesScreen
import sh.transom.settings.SettingsActions
import sh.transom.settings.SettingsScreen

/**
 * The signed-in app: the inbox, or the last of the [places] over it, which the system's back
 * gesture closes. [go] moves to a new list of places, and an empty list is the inbox. [settings] is
 * what Settings and the devices list do outside the account, such as signing out.
 *
 * The app has a handful of screens, each with one argument at most, so the navigator is this list
 * rather than navigation-compose, which would add routes and a library for what a list of [Place]s
 * and a [BackHandler] do. Each place's saved state is held under the place as its key, so the inbox
 * keeps its scroll position while an exchange is open.
 *
 * While the app is on screen, at least started, it follows [account]'s live feed, as the web inbox
 * follows its own, and stops when the app leaves the screen.
 */
@Composable
fun SignedInScreens(
    account: Account,
    places: List<Place>,
    go: (List<Place>) -> Unit,
    settings: SettingsActions,
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
    BackHandler(enabled = places.isNotEmpty()) { go(places.dropLast(1)) }
    val open = { place: Place -> go(places + place) }
    val scope = rememberCoroutineScope()
    val actions =
        remember(account, places) {
            InboxActions(
                onOpen = { open(Place.Exchange(it)) },
                onRead = { scope.launch { account.outbox.markAll(it) } },
                onUnread = { ids ->
                    scope.launch { ids.forEach { account.outbox.mark(it, read = false) } }
                },
                onAnswer = { id, key -> scope.launch { account.outbox.reply(id, choice = key) } },
            )
        }
    val saved = rememberSaveableStateHolder()
    val here = places.lastOrNull()
    saved.SaveableStateProvider(here ?: INBOX) {
        when (here) {
            null -> InboxScreen(account.name, inbox, account.pictures, actions, modifier)
            is Place.Exchange ->
                ExchangeScreen(account, here.id, { open(Place.Members(it)) }, modifier)
            // Leaving a group takes its exchanges with it, so the inbox is where to go next.
            is Place.Members -> MembersScreen(account, here.group, { go(emptyList()) }, modifier)
            Place.Settings -> SettingsScreen(account, settings, open, modifier)
            Place.Devices -> DevicesScreen(account, settings.onSignOut, modifier)
            // The licenses are the place left. An else rather than naming it: a when that names
            // every place compiles a branch for a place that cannot exist, which no test reaches.
            else -> LicensesScreen(modifier)
        }
    }
}

/** The inbox's key among the saved states, which no [Place] is. */
private const val INBOX = "inbox"
