package sh.transom.signin

import java.io.File
import javax.crypto.SecretKey
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json

/**
 * The session and any sign-in in progress, each sealed in a file of [directory] with [key]. The app
 * keeps them in `noBackupFilesDir`, so neither leaves the phone in a backup, where the key could
 * not follow.
 *
 * [key] is asked for once, when a file is first read or written.
 *
 * The app has one store for its process, and [changes] follows its session, so a screen hears when
 * the session is forgotten in the background, as when a request learns that the server ended it.
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

    private val current by lazy { MutableStateFlow(session.read()) }

    /** The session the app signed in with, or null when it is signed out. */
    fun session(): Session? = session.read()

    /** The session as it changes, from the one stored when it is first asked for. */
    val changes: StateFlow<Session?>
        get() = current

    /** Why the server ended the last session this process forgot, or null when it did not. */
    var ended: Problem.Ended? = null
        private set

    fun keep(signedIn: Session) {
        session.write(signedIn)
        ended = null
        current.value = signedIn
    }

    /** Forgets the session, which the server [ended] when the app did not sign out itself. */
    fun forget(ended: Problem.Ended? = null) {
        session.delete()
        this.ended = ended
        current.value = null
    }

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
