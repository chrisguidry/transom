package sh.transom.pictures

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import java.io.File
import java.security.MessageDigest
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import sh.transom.sync.Client

/**
 * The pictures of identities, fetched with [client]'s session and kept on disk in [directory], one
 * file for each URL.
 *
 * A picture's URL carries a hash of its bytes, so a new picture has a new URL, and a file kept for
 * a URL never goes stale. The directory is under the app's cache, which Android may empty when it
 * runs short of space, and signing out deletes it.
 */
class Pictures(private val client: Client, private val directory: File) {
    /**
     * The picture at [url], decoded to at least [size] pixels a side where it has them, or `null`
     * when there is no picture, or it cannot be fetched or decoded. A notification or a row then
     * shows the initials in its place.
     */
    suspend fun bitmap(url: String?, size: Int): Bitmap? {
        val bytes = url?.let { bytes(it) } ?: return null
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        // A picture is at most 1024 pixels a side, and an icon is far smaller, so it is decoded at
        // the largest fraction of its size that still covers the icon.
        val options =
            BitmapFactory.Options().apply {
                inSampleSize = Integer.highestOneBit((bounds.outWidth / size).coerceAtLeast(1))
            }
        return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
    }

    private suspend fun bytes(url: String): ByteArray? =
        withContext(Dispatchers.IO) {
            val kept = File(directory, name(url))
            if (kept.exists()) return@withContext kept.readBytes()
            val fetched =
                try {
                    client.picture(url)
                } catch (_: Exception) {
                    // Cancelling ends the fetch, and any other failure, from a lost connection to a
                    // picture since removed, shows the initials.
                    currentCoroutineContext().ensureActive()
                    return@withContext null
                }
            // Written whole under another name first, so a reader never finds half a picture.
            directory.mkdirs()
            val partial = File.createTempFile("picture", null, directory)
            partial.writeBytes(fetched)
            partial.renameTo(kept)
            fetched
        }

    private fun name(url: String): String =
        MessageDigest.getInstance("SHA-256").digest(url.toByteArray()).joinToString("") {
            "%02x".format(it)
        }
}
