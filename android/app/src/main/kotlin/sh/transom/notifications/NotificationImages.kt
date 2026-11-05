package sh.transom.notifications

import android.content.Context
import android.net.Uri
import androidx.core.content.FileProvider
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import sh.transom.exchange.Exchange
import sh.transom.sync.Client

/**
 * The image a notification shows: the first image attached to its message, fetched with [client]'s
 * session and kept in the app's cache while the notification is up.
 *
 * Android's system UI draws the image, and cannot read the app's files, so each is offered through
 * the app's [FileProvider]. Android grants the system UI access to each URI a notification names as
 * it posts the notification, so nothing else can read it.
 */
internal class NotificationImages(private val context: Context, private val client: Client) {
    private val directory = File(context.cacheDir, DIRECTORY)

    /**
     * The type and URI of [exchange]'s first image, or `null` when it has none, or it cannot be
     * fetched. The notification then shows the text alone.
     */
    suspend fun first(exchange: Exchange): Pair<String, Uri>? {
        val image =
            exchange.message.attachments.firstOrNull { it.contentType.startsWith("image/") }
                ?: return null
        val kept = File(directory, exchange.id)
        return withContext(Dispatchers.IO) {
            if (!kept.exists()) {
                val bytes =
                    try {
                        client.attachment(exchange.id, image.name)
                    } catch (_: Exception) {
                        // Cancelling ends the fetch, and any other failure leaves the image out.
                        currentCoroutineContext().ensureActive()
                        return@withContext null
                    }
                directory.mkdirs()
                val partial = File.createTempFile("image", null, directory)
                partial.writeBytes(bytes)
                partial.renameTo(kept)
            }
            image.contentType to FileProvider.getUriForFile(context, authority(context), kept)
        }
    }

    /** Deletes exchange [id]'s image, once its notification is gone. */
    fun forget(id: String) {
        File(directory, id).delete()
    }
}

/** The FileProvider's authority, which the manifest names after the application id. */
internal fun authority(context: Context) = "${context.packageName}.images"

/**
 * The directory under the app's cache that the FileProvider offers, as `res/xml/images.xml` says.
 */
internal const val DIRECTORY = "notification-images"
