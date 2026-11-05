package sh.transom.signin

import com.google.zxing.BarcodeFormat
import com.google.zxing.qrcode.QRCodeWriter
import java.nio.ByteBuffer
import kotlin.random.Random
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized

/**
 * A camera's frame with a QR code in it gives the code's text, and a frame without one, nothing.
 */
class QrCodeTest {
    @Test
    fun readsTheCodeInAFrame() {
        assertEquals("http://127.0.0.1:8080", qrText(frameShowing("http://127.0.0.1:8080")))
    }

    @Test
    fun readsACodeWhoseRowsArePadded() {
        assertEquals(
            "https://transom.example.com",
            qrText(frameShowing("https://transom.example.com", padding = 64)),
        )
    }

    @Test
    fun readsNothingInAFrameOfNoise() {
        val noise = Random(9).nextBytes(FRAME * FRAME)
        assertNull(qrText(Frame(ByteBuffer.wrap(noise), FRAME, FRAME, FRAME)))
    }

    @Test
    fun readsNothingInABlankFrame() {
        val blank = ByteArray(FRAME * FRAME) { WHITE }
        assertNull(qrText(Frame(ByteBuffer.wrap(blank), FRAME, FRAME, FRAME)))
    }
}

/** Only an `http` or `https` URL in a code is a server's address. */
@RunWith(Parameterized::class)
class ScannedServerTest(private val text: String, private val expected: String?) {
    @Test
    fun accepts() {
        assertEquals(expected, scannedServer(text))
    }

    companion object {
        @JvmStatic
        @Parameterized.Parameters(name = "{0} -> {1}")
        fun cases(): List<Array<Any?>> =
            listOf(
                arrayOf("http://127.0.0.1:8080", "http://127.0.0.1:8080"),
                arrayOf("https://transom.example.com/", "https://transom.example.com"),
                arrayOf(" https://transom.example.com\n", "https://transom.example.com"),
                arrayOf("transom.example.com", null),
                arrayOf("ftp://transom.example.com", null),
                arrayOf("mailto:chris@example.com", null),
                arrayOf("WIFI:S:home;T:WPA;P:secret;;", null),
                arrayOf("", null),
            )
    }
}

private const val FRAME = 480
private const val WHITE = 0xff.toByte()

/**
 * A camera's luminance plane of [FRAME] pixels square showing a QR code of [text], dark on light as
 * the web draws it, with [padding] bytes after each row as a camera's buffer may have.
 */
private fun frameShowing(text: String, padding: Int = 0): Frame {
    val code = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, FRAME, FRAME)
    val stride = FRAME + padding
    val plane = ByteArray(stride * FRAME) { WHITE }
    for (y in 0 until FRAME) {
        for (x in 0 until FRAME) {
            if (code[x, y]) plane[y * stride + x] = 0
        }
    }
    return Frame(ByteBuffer.wrap(plane), stride, FRAME, FRAME)
}
