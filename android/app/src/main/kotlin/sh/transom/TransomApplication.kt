package sh.transom

import android.app.Application
import javax.crypto.SecretKey
import okhttp3.OkHttpClient
import sh.transom.signin.AndroidKeystoreOnly
import sh.transom.signin.SessionStore
import sh.transom.signin.SignInRequests
import sh.transom.signin.androidKeystoreKey

/**
 * The app's process: the one session store and HTTP client every screen shares. It is open so the
 * unit tests can seal the session with a key of their own, since Robolectric has no Keystore.
 */
open class TransomApplication : Application() {
    val sessions: SessionStore by lazy { SessionStore(noBackupFilesDir, ::sessionKey) }

    val signInRequests: SignInRequests by lazy { SignInRequests(http) }

    private val http: OkHttpClient by lazy { OkHttpClient() }

    @AndroidKeystoreOnly protected open fun sessionKey(): SecretKey = androidKeystoreKey()
}
