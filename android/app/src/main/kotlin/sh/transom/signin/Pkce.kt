package sh.transom.signin

import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Base64

/** A PKCE verifier, as RFC 7636 asks: 32 random bytes in base64url, without padding. */
internal fun newVerifier(): String = base64url(ByteArray(32).also { SecureRandom().nextBytes(it) })

/** The S256 challenge of [verifier], which the server keeps until the app proves it with this. */
internal fun challengeOf(verifier: String): String =
    base64url(MessageDigest.getInstance("SHA-256").digest(verifier.toByteArray(Charsets.US_ASCII)))

private fun base64url(bytes: ByteArray): String =
    Base64.getUrlEncoder().withoutPadding().encodeToString(bytes)
