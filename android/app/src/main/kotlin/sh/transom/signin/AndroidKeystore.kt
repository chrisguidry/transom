package sh.transom.signin

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyStore
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey

/**
 * Code that runs only against the Android Keystore. Robolectric has no AndroidKeyStore provider, so
 * the unit tests cannot run it, and the build file excludes it from Kover's coverage. Keep each
 * such function to the calls that reach the Keystore; the logic around them takes the key as a
 * parameter, so the tests run it with a key of their own.
 */
@Target(AnnotationTarget.FUNCTION)
@Retention(AnnotationRetention.BINARY)
annotation class AndroidKeystoreOnly

/**
 * The AES-256-GCM key that seals the session, made in the Android Keystore the first time it is
 * asked for. The Keystore never lets the key's bytes out of it, so a copy of the app's files is no
 * use without this phone.
 */
@AndroidKeystoreOnly
internal fun androidKeystoreKey(): SecretKey {
    val keystore = KeyStore.getInstance(PROVIDER).apply { load(null) }
    (keystore.getKey(ALIAS, null) as SecretKey?)?.let {
        return it
    }
    val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, PROVIDER)
    generator.init(
        KeyGenParameterSpec.Builder(
                ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .build()
    )
    return generator.generateKey()
}

private const val PROVIDER = "AndroidKeyStore"
private const val ALIAS = "session"
