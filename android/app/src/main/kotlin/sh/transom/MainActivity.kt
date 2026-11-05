package sh.transom

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.Surface
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import sh.transom.delivery.OPEN_SETTINGS
import sh.transom.notifications.NotificationPermission
import sh.transom.notifications.OPEN_EXCHANGE
import sh.transom.settings.SettingsActions
import sh.transom.signin.SignIn
import sh.transom.signin.SignInScreen
import sh.transom.signin.SignInState
import sh.transom.theme.TransomTheme

/**
 * The app's one activity: the inbox when signed in, and the sign-in screen when not. It is
 * `singleTask` in the manifest, so the App Link that ends a sign-in comes back to it through
 * [onNewIntent] and closes the Custom Tab above it, as does a notification that opens an exchange.
 */
class MainActivity : ComponentActivity() {
    private val app: TransomApplication
        get() = application as TransomApplication

    private val signIn: SignIn by viewModels {
        viewModelFactory {
            initializer {
                SignIn(
                    app.sessions,
                    app.signInRequests,
                    signedIn = app::signedIn,
                    signedOut = app::signedOut,
                    renamed = app::restarted,
                )
            }
        }
    }

    private val notifications = NotificationPermission(this)

    /** The place a notification asked to open, until the screens open it. */
    private var requested by mutableStateOf<Place?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        // The page's paper runs under the system bars, and the content
        // keeps clear of them and of any cutout.
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        // A recreated activity has the intent it began with, which it read the first time.
        if (savedInstanceState == null) arrived(intent)
        setContent {
            TransomTheme {
                Surface(Modifier.fillMaxSize()) {
                    Column(Modifier.windowInsetsPadding(WindowInsets.safeDrawing)) {
                        val state = signIn.state
                        // The places open over the inbox, if any, kept when the activity is made
                        // again, as after the phone turns.
                        var places by rememberSaveable { mutableStateOf(emptyList<Place>()) }
                        LaunchedEffect(requested) {
                            requested?.let {
                                places = listOf(it)
                                requested = null
                            }
                        }
                        val signedIn = state is SignInState.SignedIn
                        LaunchedEffect(signedIn) { if (signedIn) notifications.askOnce() }
                        val signOut = {
                            places = emptyList()
                            signIn.signOut()
                        }
                        TopBar(
                            (state as? SignInState.SignedIn)?.session?.name,
                            notifications.on,
                            onHome = { places = emptyList() },
                            onBell = notifications::pressed,
                            onSettings = { places = listOf(Place.Settings) },
                            onSignOut = signOut,
                        )
                        // The web UI's main column is 42rem at most, so a
                        // tablet reads the same line lengths.
                        val column =
                            Modifier.widthIn(max = 672.dp)
                                .fillMaxWidth()
                                .align(Alignment.CenterHorizontally)
                        // An if rather than a when: a when over the sealed
                        // state compiles a branch for a state that cannot
                        // exist, which no test can reach.
                        if (state is SignInState.SignedIn) {
                            app.account()?.let {
                                val settings =
                                    SettingsActions(
                                        notifications.on,
                                        onNotificationSettings = notifications::openSettings,
                                        onRenamed = signIn::renamed,
                                        onSignOut = signOut,
                                        googlePlay = app.googlePlay,
                                        delivery = app.delivery(),
                                        onDelivery = app::deliverBy,
                                    )
                                SignedInScreens(
                                    it,
                                    places,
                                    { next -> places = next },
                                    settings,
                                    column,
                                )
                            }
                        } else if (state is SignInState.SignedOut) {
                            SignInScreen(
                                state.problem,
                                state.working,
                                onContinue = { signIn.begin(it, ::openCustomTab) },
                                modifier = column,
                            )
                        }
                    }
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        arrived(intent)
    }

    /**
     * Signs in from an App Link, or opens the exchange a notification names, or Settings, from the
     * notification that says delivery is failing.
     */
    private fun arrived(intent: Intent) {
        signIn.arrived(intent)
        intent.getStringExtra(OPEN_EXCHANGE)?.let { requested = Place.Exchange(it) }
        if (intent.getBooleanExtra(OPEN_SETTINGS, false)) requested = Place.Settings
    }

    override fun onStart() {
        super.onStart()
        // The background connection may have stopped, as when the person stops the app from its
        // notification, so coming to the front starts it again, when the phone delivers by it.
        if (signIn.state is SignInState.SignedIn) app.signedIn()
    }

    override fun onResume() {
        super.onResume()
        signIn.resumed()
        notifications.refresh()
    }

    private fun openCustomTab(page: Uri): Boolean =
        try {
            CustomTabsIntent.Builder().build().launchUrl(this, page)
            true
        } catch (_: ActivityNotFoundException) {
            false
        }
}
