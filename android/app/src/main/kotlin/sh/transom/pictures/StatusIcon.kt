package sh.transom.pictures

import android.graphics.Bitmap
import android.graphics.Color
import androidx.core.graphics.createBitmap
import androidx.core.graphics.scale
import kotlin.math.abs
import kotlin.math.max

/**
 * A status bar icon of [size] pixels a side, drawn from a sender's [picture], or `null` when the
 * picture has no mark that can stand alone.
 *
 * Android draws a notification's small icon from its alpha alone, so a picture as it is shows as a
 * solid square. Most senders' pictures are a mark on one flat color, as the relays' are, so the
 * background is the color all around the picture's edge, and each pixel's opacity in the icon is
 * how far its color is from that background: the background goes clear, and the mark stays. A
 * picture with no one color around its edge, such as a photo, would come out as noise, and one with
 * nothing on its background as nothing, so neither has an icon.
 */
internal fun statusIcon(picture: Bitmap, size: Int): Bitmap? {
    val scaled = picture.scale(size, size)
    val pixels = IntArray(size * size)
    scaled.getPixels(pixels, 0, size, 0, 0, size, size)
    val edge = edge(pixels, size)
    val background = edge.groupingBy { it }.eachCount().maxBy { it.value }.key
    val flat = edge.count { distance(it, background) <= NEAR }
    if (flat < edge.size * FLAT) return null
    val distances = pixels.map { distance(it, background) }
    val farthest = distances.max()
    if (farthest < CONTRAST) return null
    val icon = createBitmap(size, size)
    icon.setPixels(
        distances.map { Color.argb(it * 255 / farthest, 255, 255, 255) }.toIntArray(),
        0,
        size,
        0,
        0,
        size,
        size,
    )
    return icon
}

/** The pixels around the edge of a square of [size] pixels a side. */
private fun edge(pixels: IntArray, size: Int): List<Int> =
    (0 until size).flatMap { i ->
        listOf(
            pixels[i],
            pixels[(size - 1) * size + i],
            pixels[i * size],
            pixels[i * size + size - 1],
        )
    }

/**
 * How far apart two colors are: the largest difference in any channel, alpha among them, so a mark
 * on a clear background stands out by its alpha.
 */
private fun distance(a: Int, b: Int): Int =
    max(
        max(abs(Color.alpha(a) - Color.alpha(b)), abs(Color.red(a) - Color.red(b))),
        max(abs(Color.green(a) - Color.green(b)), abs(Color.blue(a) - Color.blue(b))),
    )

/** How close to the background an edge pixel must be to count as background. */
private const val NEAR = 24

/** The share of the edge that must be background for the picture to have a flat one. */
private const val FLAT = 0.9

/** How far the mark must be from the background, so a faint texture is not taken for a mark. */
private const val CONTRAST = 64
