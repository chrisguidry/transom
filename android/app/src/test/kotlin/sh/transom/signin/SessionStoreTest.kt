package sh.transom.signin

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class SessionStoreTest {
    @get:Rule val folder = TemporaryFolder()

    private val key = aesKey()
    private var keysAskedFor = 0
    private val store by lazy {
        SessionStore(folder.root) {
            keysAskedFor++
            key
        }
    }

    @Test
    fun keepsTheSessionAcrossStores() {
        store.keep(ALICE)
        assertEquals(ALICE, SessionStore(folder.root) { key }.session())
    }

    @Test
    fun isSignedOutAtFirst() {
        assertNull(store.session())
    }

    @Test
    fun forgetsTheSession() {
        store.keep(ALICE)
        store.forget()
        assertNull(store.session())
    }

    @Test
    fun keepsASignInInProgressApart() {
        store.begin(PendingSignIn("https://transom.example.com", "verifier"))
        assertEquals(
            PendingSignIn("https://transom.example.com", "verifier"),
            SessionStore(folder.root) { key }.pending(),
        )
    }

    @Test
    fun finishesASignInInProgress() {
        store.begin(PendingSignIn("https://transom.example.com", "verifier"))
        store.finish()
        assertNull(store.pending())
    }

    @Test
    fun asksForTheKeyOnce() {
        store.keep(ALICE)
        store.session()
        store.pending()
        assertEquals(1, keysAskedFor)
    }

    @Test
    fun readsSomethingElseAsSignedOut() {
        SealedFile(File(folder.root, "session")) { key }.write("not a session".toByteArray())
        assertNull(store.session())
    }

    private companion object {
        val ALICE = Session("https://transom.example.com", "transom_token", "alice")
    }
}
