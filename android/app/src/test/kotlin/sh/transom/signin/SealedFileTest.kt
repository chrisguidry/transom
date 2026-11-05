package sh.transom.signin

import java.io.File
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The encryption the Keystore's key runs, with a key the test made in its place. */
@RunWith(RobolectricTestRunner::class)
class SealedFileTest {
    @get:Rule val folder = TemporaryFolder()

    private val key = aesKey()
    private val file by lazy { File(folder.root, "secret") }

    @Test
    fun readsWhatWasWritten() {
        SealedFile(file) { key }.write(SECRET)
        assertArrayEquals(SECRET, SealedFile(file) { key }.read())
    }

    @Test
    fun keepsNoPlaintextOnDisk() {
        SealedFile(file) { key }.write(SECRET)
        assertFalse(file.readText().contains(SECRET.decodeToString()))
    }

    @Test
    fun readsAsAbsentBeforeAnyWrite() {
        assertNull(SealedFile(file) { key }.read())
    }

    @Test
    fun readsAsAbsentOnceDeleted() {
        SealedFile(file) { key }.apply { write(SECRET) }.delete()
        assertNull(SealedFile(file) { key }.read())
    }

    @Test
    fun readsAsAbsentUnderAnotherKey() {
        SealedFile(file) { key }.write(SECRET)
        assertNull(SealedFile(file) { aesKey() }.read())
    }

    @Test
    fun readsAsAbsentCopiedUnderAnotherName() {
        SealedFile(file) { key }.write(SECRET)
        val copy = file.copyTo(File(folder.root, "other"))
        assertNull(SealedFile(copy) { key }.read())
    }

    @Test
    fun readsAsAbsentWhenEmpty() {
        file.writeBytes(ByteArray(0))
        assertNull(SealedFile(file) { key }.read())
    }

    @Test
    fun readsAsAbsentWhenCutShort() {
        file.writeBytes(byteArrayOf(12, 1, 2, 3))
        assertNull(SealedFile(file) { key }.read())
    }

    private companion object {
        val SECRET = "transom_a-session-token".toByteArray()
    }
}

internal fun aesKey(): SecretKey = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
