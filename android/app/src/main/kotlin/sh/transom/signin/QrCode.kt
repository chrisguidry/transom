package sh.transom.signin

import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.ReaderException
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader
import java.nio.ByteBuffer
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

/**
 * One frame from the camera: its luminance plane, the first plane of a YUV image, which is all a QR
 * code needs. Each row is [rowStride] bytes, which may be more than [width] when the camera pads
 * its rows.
 */
internal class Frame(
    val luminance: ByteBuffer,
    val rowStride: Int,
    val width: Int,
    val height: Int,
)

/**
 * The text of the QR code in [frame], or null when ZXing finds none. A code reads the same at any
 * rotation, so the frame is read as the camera sends it.
 */
internal fun qrText(frame: Frame): String? {
    // The buffer is the camera's and goes back to it, so ZXing reads a copy.
    val bytes = ByteArray(frame.luminance.remaining()).also { frame.luminance.get(it) }
    // The plane's last row may stop at the width rather than the stride, so the source is told the
    // plane's rows are rowStride wide and is cropped to the width.
    val source =
        PlanarYUVLuminanceSource(
            bytes,
            frame.rowStride,
            frame.height,
            0,
            0,
            frame.width,
            frame.height,
            false,
        )
    return try {
        QRCodeReader().decode(BinaryBitmap(HybridBinarizer(source)), HINTS).text
    } catch (_: ReaderException) {
        null
    }
}

// The camera sends frames faster than they can be read, and the ones that arrive while one is
// being read are dropped, so each frame can take the time to look harder.
private val HINTS = mapOf(DecodeHintType.TRY_HARDER to true)

/**
 * The server's URL in a scanned code's [text], as the web's devices page draws it: an `http` or
 * `https` URL, without a trailing slash. Null for anything else, such as a Wi-Fi network's code,
 * since a code is not typed and a URL without its scheme did not come from transom.
 */
internal fun scannedServer(text: String): String? =
    text.trim().toHttpUrlOrNull()?.let { serverUrl(it.toString()) }
