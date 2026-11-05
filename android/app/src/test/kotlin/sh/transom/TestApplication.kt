package sh.transom

import javax.crypto.KeyGenerator
import javax.crypto.SecretKey

/** The app as the tests run it, sealing the session with an AES-256 key from the JVM. */
class TestApplication : TransomApplication() {
    override fun sessionKey(): SecretKey = key

    private val key = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
}
