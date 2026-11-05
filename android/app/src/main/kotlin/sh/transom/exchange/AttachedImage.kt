package sh.transom.exchange

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.unit.dp
import sh.transom.sync.Client

/**
 * One of exchange [id]'s own images, fetched with [client]'s token, described by [alt]. It is as
 * wide as the image, or as the column when the image is wider, as `.body img` is on the web. It
 * takes no room while it loads, and shows [alt] when it cannot be loaded.
 */
@Composable
fun AttachedImage(client: Client, id: String, attachment: Attachment, alt: String) {
    val image by
        produceState<Loaded?>(null, client, id, attachment.name) {
            // Any failure, from a lost connection to bytes that are no image, leaves the image
            // unshown, as a browser shows a broken image by its description.
            value =
                Loaded(runCatching { decode(client.attachment(id, attachment.name)) }.getOrNull())
        }
    val loaded = image ?: return
    val bitmap = loaded.bitmap
    if (bitmap == null) {
        Text(alt, color = MaterialTheme.colorScheme.onSurfaceVariant)
        return
    }
    Image(
        bitmap = bitmap,
        contentDescription = alt,
        modifier =
            Modifier.widthIn(max = bitmap.width.dp)
                .fillMaxWidth()
                .aspectRatio(bitmap.width.toFloat() / bitmap.height)
                .clip(RoundedCornerShape(4.dp)),
    )
}

/** An image once its fetch has finished: the [bitmap], or `null` when there is none to show. */
private class Loaded(val bitmap: ImageBitmap?)

/**
 * The image in [bytes], decoded at a fraction of its size when it is wider than [WIDEST] pixels,
 * since an attachment may be up to 5 MB of compressed pixels and the screen shows far fewer.
 */
private fun decode(bytes: ByteArray): ImageBitmap {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
    val options =
        BitmapFactory.Options().apply { inSampleSize = (bounds.outWidth / WIDEST).coerceAtLeast(1) }
    return checkNotNull(BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options))
        .asImageBitmap()
}

private const val WIDEST = 2048
