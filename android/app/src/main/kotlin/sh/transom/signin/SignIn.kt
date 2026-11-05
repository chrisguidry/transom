package sh.transom.signin

import android.content.Intent
import android.net.Uri
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.net.toUri
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Whether the app is signed in, and if not, what the sign-in screen shows. */
sealed interface SignInState {
    data class SignedIn(val session: Session) : SignInState

    /** Signed out, with the [problem] of the last try, while [working] on the next one. */
    data class SignedOut(val problem: Problem? = null, val working: Boolean = false) : SignInState
}

/**
 * Signing in and out, from the server's URL through the Custom Tab to the session, and back.
 *
 * A sign-in in progress is kept in [store], so a callback that arrives after Android stopped the
 * app still redeems its code. The activity reports each intent that arrives, with [arrived], and
 * each time it comes back to the front, with [resumed], which is how the app learns that the Custom
 * Tab closed without a callback.
 *
 * [signedIn] runs once a sign-in finishes, and [signedOut] once the session is forgotten, so the
 * app can begin and end what it does as that session. [renamed] runs once the session's name
 * changes, so the app can begin again as the session it now is.
 */
class SignIn(
    private val store: SessionStore,
    private val requests: SignInRequests,
    private val signedIn: () -> Unit,
    private val signedOut: suspend () -> Unit,
    private val renamed: () -> Unit,
) : ViewModel() {
    var state: SignInState by
        mutableStateOf(store.session()?.let(SignInState::SignedIn) ?: SignInState.SignedOut())
        private set

    /**
     * Begins signing in to the server [typed], once the server answers as transom, by handing
     * [open] the sign-in page for the Custom Tab. [open] returns false when no browser can open it.
     */
    fun begin(typed: String, open: (Uri) -> Boolean) {
        val server = serverUrl(typed) ?: return fail(Problem.NotAnAddress)
        val verifier = newVerifier()
        val challenge = challengeOf(verifier)
        state = SignInState.SignedOut(working = true)
        viewModelScope.launch {
            try {
                withContext(Dispatchers.IO) {
                    requests.check(server, challenge)
                    store.begin(PendingSignIn(server, verifier))
                }
            } catch (failure: SignInFailed) {
                return@launch fail(failure.problem)
            }
            state = SignInState.SignedOut()
            if (!open(requests.loginUrl(server, challenge).toString().toUri())) {
                store.finish()
                fail(Problem.NoBrowser)
            }
        }
    }

    /** Redeems the code of a callback to [APP_CALLBACK], and ignores any other [intent]. */
    fun arrived(intent: Intent) {
        val callback = intent.data?.takeIf { it.isAppCallback() } ?: return
        val pending = store.pending()
        store.finish()
        val code = callback.getQueryParameter("code")
        if (pending == null || code == null) return fail(Problem.Refused)
        state = SignInState.SignedOut(working = true)
        viewModelScope.launch {
            try {
                val session =
                    withContext(Dispatchers.IO) {
                        val token = requests.redeem(pending.server, code, pending.verifier)
                        val name = requests.nameOf(pending.server, token)
                        Session(pending.server, token, name).also(store::keep)
                    }
                state = SignInState.SignedIn(session)
                signedIn()
            } catch (failure: SignInFailed) {
                fail(failure.problem)
            }
        }
    }

    /**
     * The app came back to the front. A sign-in still pending then was left in the Custom Tab,
     * since a callback arrives before the app resumes and finishes the sign-in.
     */
    fun resumed() {
        val idle = (state as? SignInState.SignedOut)?.working == false
        if (idle && store.pending() != null) {
            store.finish()
            fail(Problem.Cancelled)
        }
    }

    /** Forgets the session, and ends it on the server if the server can be reached. */
    fun signOut() {
        val session = (state as? SignInState.SignedIn)?.session ?: return
        store.forget()
        state = SignInState.SignedOut()
        viewModelScope.launch { signedOut() }
        viewModelScope.launch(Dispatchers.IO) { requests.signOut(session) }
    }

    /**
     * The person renamed themselves to [name], which the server has taken: the session keeps the
     * new name, so the top bar, the feed, and everything else the app asks for use it from now on.
     */
    fun renamed(name: String) {
        val session = (state as? SignInState.SignedIn)?.session?.copy(name = name) ?: return
        store.keep(session)
        state = SignInState.SignedIn(session)
        renamed()
    }

    private fun fail(problem: Problem) {
        state = SignInState.SignedOut(problem)
    }
}

private fun Uri.isAppCallback(): Boolean {
    val callback = APP_CALLBACK.toUri()
    return scheme == callback.scheme && host == callback.host && path == callback.path
}
