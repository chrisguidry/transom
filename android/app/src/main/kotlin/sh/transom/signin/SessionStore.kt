package sh.transom.signin

import java.io.File
import javax.crypto.SecretKey
import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json

/**
 * The session and any sign-in in progress, each sealed in a file of [directory] with [key]. The app
 * keeps them in `noBackupFilesDir`, so neither leaves the phone in a backup, where the key could
 * not follow.
 *
 * [key] is asked for once, when a file is first read or written.
 */
class SessionStore(directory: File, key: () -> SecretKey) {
    private val sealedWith = lazy(key)
    private val session =
        Stored(SealedFile(File(directory, "session"), sealedWith::value), Session.serializer())
    private val pending =
        Stored(
            SealedFile(File(directory, "sign-in"), sealedWith::value),
            PendingSignIn.serializer(),
        )

    /** The session the app signed in with, or null when it is signed out. */
    fun session(): Session? = session.read()

    fun keep(signedIn: Session) = session.write(signedIn)

    fun forget() = session.delete()

    internal fun pending(): PendingSignIn? = pending.read()

    internal fun begin(signIn: PendingSignIn) = pending.write(signIn)

    internal fun finish() = pending.delete()
}

private class Stored<T>(private val file: SealedFile, private val serializer: KSerializer<T>) {
    fun read(): T? {
        val plain = file.read() ?: return null
        return try {
            Json.decodeFromString(serializer, plain.decodeToString())
        } catch (_: SerializationException) {
            null
        }
    }

    fun write(value: T) = file.write(Json.encodeToString(serializer, value).toByteArray())

    fun delete() = file.delete()
}
