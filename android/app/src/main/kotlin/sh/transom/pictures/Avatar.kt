package sh.transom.pictures

import android.graphics.Bitmap
import android.graphics.BitmapShader
import android.graphics.Canvas
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.PorterDuff
import android.graphics.PorterDuffXfermode
import android.graphics.Shader
import android.graphics.Typeface
import androidx.compose.ui.graphics.toArgb
import androidx.core.graphics.createBitmap
import sh.transom.exchange.Urgency
import sh.transom.theme.LightPalette

/**
 * A round icon of [size] pixels for a notification: [picture] cropped to a circle, or the initials
 * of [name] on a quiet circle when there is none, as the web shows a party without a picture. With
 * a [dot], a small circle of that color sits at the top right, cut out from the picture by a clear
 * ring, so it reads against any picture and any shade. The bottom right, where the web puts it, is
 * where Android draws the app's own badge over a conversation's icon, which would hide the dot.
 */
internal fun avatar(picture: Bitmap?, name: String, dot: Int?, size: Int): Bitmap {
    val icon = createBitmap(size, size)
    val canvas = Canvas(icon)
    val radius = size / 2f
    val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    if (picture != null) {
        val shader = BitmapShader(picture, Shader.TileMode.CLAMP, Shader.TileMode.CLAMP)
        shader.setLocalMatrix(
            Matrix().apply {
                setScale(size / picture.width.toFloat(), size / picture.height.toFloat())
            }
        )
        paint.shader = shader
        canvas.drawCircle(radius, radius, radius, paint)
    } else {
        paint.color = LightPalette.soft.toArgb()
        canvas.drawCircle(radius, radius, radius, paint)
        val text =
            Paint(Paint.ANTI_ALIAS_FLAG).apply {
                color = LightPalette.muted.toArgb()
                textSize = size * 0.38f
                textAlign = Paint.Align.CENTER
                typeface = Typeface.DEFAULT_BOLD
            }
        canvas.drawText(initials(name), radius, radius - (text.ascent() + text.descent()) / 2, text)
    }
    if (dot != null) {
        val dotRadius = size * 0.15f
        val x = size - dotRadius
        val clear =
            Paint(Paint.ANTI_ALIAS_FLAG).apply {
                xfermode = PorterDuffXfermode(PorterDuff.Mode.CLEAR)
            }
        canvas.drawCircle(x, dotRadius, dotRadius + size * 0.05f, clear)
        canvas.drawCircle(
            x,
            dotRadius,
            dotRadius,
            Paint(Paint.ANTI_ALIAS_FLAG).apply { color = dot },
        )
    }
    return icon
}

/**
 * The color of the dot on a sender's icon for a message of [urgency]: red for `urgent` and `high`,
 * amber for `normal`, and none below, so a glance at the shade tells what needs attention.
 */
internal fun dotOf(urgency: Urgency): Int? =
    when (urgency) {
        Urgency.URGENT,
        Urgency.HIGH -> RED
        Urgency.NORMAL -> AMBER
        Urgency.LOW,
        Urgency.VERY_LOW -> null
    }

private const val RED = 0xFFC2412D.toInt()
private const val AMBER = 0xFFB7791F.toInt()

/**
 * Up to two letters for a circle that stands in for a missing picture, as the web makes them: the
 * first letter of each of the first two words of [name], where a handle's words are split at its
 * dots and hyphens, and `?` when there are none.
 */
fun initials(name: String): String {
    val initials =
        name
            .removePrefix("@")
            .split(' ', '\t', '\n', '-', '.')
            .mapNotNull { it.firstOrNull() }
            .take(2)
            .joinToString("") { it.uppercase() }
    return initials.ifEmpty { "?" }
}
