package sh.transom.signin

import android.util.AtomicFile
import java.io.File
import java.io.FileNotFoundException
import java.security.GeneralSecurityException
import javax.crypto.Cipher
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * A file whose contents are encrypted with AES-256-GCM under [key]: the IV's length in one byte,
 * the IV, then the ciphertext and its tag. The file's name is authenticated with it, so one sealed
 * file copied over another fails to open instead of reading as the other.
 *
 * The cipher makes the IV itself, because a key in the Android Keystore refuses an IV it was given.
 * A file that fails to open, such as one sealed by a key the Keystore no longer has, reads as
 * absent, and the app signs in again.
 */
internal class SealedFile(file: File, private val key: () -> SecretKey) {
    private val atomic = AtomicFile(file)
    private val name = file.name.toByteArray()

    fun read(): ByteArray? {
        val sealed =
            try {
                atomic.readFully()
            } catch (_: FileNotFoundException) {
                return null
            }
        val ivLength = sealed.firstOrNull()?.toInt() ?: return null
        if (ivLength !in 1 until sealed.size) return null
        return try {
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(TAG_BITS, sealed, 1, ivLength))
            cipher.updateAAD(name)
            cipher.doFinal(sealed, 1 + ivLength, sealed.size - 1 - ivLength)
        } catch (_: GeneralSecurityException) {
            null
        }
    }

    /**
     * Replaces the file's contents with [plain], sealed. The new file takes the old one's place in
     * one rename, so a crash midway leaves the old contents whole.
     */
    fun write(plain: ByteArray) {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, key())
        cipher.updateAAD(name)
        val sealed = cipher.doFinal(plain)
        atomic.baseFile.parentFile?.mkdirs()
        val out = atomic.startWrite()
        out.write(cipher.iv.size)
        out.write(cipher.iv)
        out.write(sealed)
        atomic.finishWrite(out)
    }

    fun delete() = atomic.delete()

    private companion object {
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val TAG_BITS = 128
    }
}
