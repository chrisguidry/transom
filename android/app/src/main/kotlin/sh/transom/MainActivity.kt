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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import sh.transom.inbox.InboxScreen
import sh.transom.signin.SignIn
import sh.transom.signin.SignInScreen
import sh.transom.signin.SignInState
import sh.transom.theme.TransomTheme

/**
 * The app's one activity: the inbox when signed in, and the sign-in screen when not. It is
 * `singleTask` in the manifest, so the App Link that ends a sign-in comes back to it through
 * [onNewIntent] and closes the Custom Tab above it.
 */
class MainActivity : ComponentActivity() {
    private val signIn: SignIn by viewModels {
        val app = application as TransomApplication
        viewModelFactory { initializer { SignIn(app.sessions, app.signInRequests) } }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        // The page's paper runs under the system bars, and the content
        // keeps clear of them and of any cutout.
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        // A recreated activity has the intent it began with, which it read the first time.
        if (savedInstanceState == null) signIn.arrived(intent)
        setContent {
            TransomTheme {
                Surface(Modifier.fillMaxSize()) {
                    Column(Modifier.windowInsetsPadding(WindowInsets.safeDrawing)) {
                        val state = signIn.state
                        TopBar((state as? SignInState.SignedIn)?.session?.name, signIn::signOut)
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
                            InboxScreen(state.session.name, column)
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
        signIn.arrived(intent)
    }

    override fun onResume() {
        super.onResume()
        signIn.resumed()
    }

    private fun openCustomTab(page: Uri): Boolean =
        try {
            CustomTabsIntent.Builder().build().launchUrl(this, page)
            true
        } catch (_: ActivityNotFoundException) {
            false
        }
}
