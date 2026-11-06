package sh.transom.pictures

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import androidx.core.graphics.createBitmap
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.GraphicsMode

/** The status bar icon the app draws from a sender's picture. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class StatusIconTest {
    @Test
    fun aGlyphOnAFlatBackgroundIsOpaqueAndItsBackgroundClear() =
        assertEquals(
            listOf(listOf(255, 0), listOf(255, 0), listOf(255, 0)),
            listOf(
                    glyph(Color.rgb(0x5f, 0x8a, 0x3a), Color.WHITE),
                    glyph(Color.WHITE, Color.BLACK),
                    glyph(Color.TRANSPARENT, Color.WHITE),
                )
                .map { opacity(checkNotNull(statusIcon(it, SIZE))) },
        )

    @Test
    fun theIconIsWhite() =
        assertEquals(
            Color.WHITE,
            checkNotNull(statusIcon(glyph(Color.WHITE, Color.BLACK), SIZE))
                .getPixel(SIZE / 2, SIZE / 2),
        )

    @Test
    fun theIconIsTheSizeAskedFor() =
        assertEquals(SIZE, statusIcon(glyph(Color.BLUE, Color.WHITE, side = 256), SIZE)?.width)

    @Test
    fun aPictureWithNothingOnItsBackgroundHasNoIcon() =
        assertNull(statusIcon(glyph(Color.BLUE, Color.BLUE), SIZE))

    @Test fun aPictureWithNoFlatBackgroundHasNoIcon() = assertNull(statusIcon(gradient(), SIZE))
}

/** A picture like a relay's: [background] edge to edge, with a [mark] circle in its middle. */
fun glyph(background: Int, mark: Int, side: Int = 128): Bitmap =
    createBitmap(side, side).also {
        val canvas = Canvas(it)
        canvas.drawColor(background, android.graphics.PorterDuff.Mode.SRC)
        canvas.drawCircle(side / 2f, side / 2f, side / 4f, Paint().apply { color = mark })
    }

/** A picture like a photo's, with no one color around its edges. */
private fun gradient(): Bitmap =
    createBitmap(128, 128).also {
        for (x in 0 until 128) for (y in 0 until 128) it.setPixel(x, y, Color.rgb(x * 2, y, 90))
    }

/** The alpha of [icon]'s middle and of its corner. */
private fun opacity(icon: Bitmap) =
    listOf(Color.alpha(icon.getPixel(SIZE / 2, SIZE / 2)), Color.alpha(icon.getPixel(1, 1)))

private const val SIZE = 96
