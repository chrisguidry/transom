package sh.transom.signin

import kotlinx.serialization.Serializable

/**
 * A signed-in app: the transom server at [server], such as `https://transom.example.com` with no
 * trailing slash, the session's bearer [token], which starts with `transom_`, and the `@name` it
 * signs in as.
 */
@Serializable data class Session(val server: String, val token: String, val name: String)

/**
 * A sign-in the app began and the Custom Tab has not finished: the [server] it began on, and the
 * PKCE [verifier] behind the challenge it sent there. It is kept on disk, so the callback still
 * redeems its code when Android stopped the app while the Custom Tab was open.
 */
@Serializable internal data class PendingSignIn(val server: String, val verifier: String)
