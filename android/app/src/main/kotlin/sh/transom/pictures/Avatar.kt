package sh.transom.pictures

import android.graphics.Bitmap
import android.graphics.BitmapShader
import android.graphics.Canvas
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Shader
import android.graphics.Typeface
import androidx.compose.ui.graphics.toArgb
import androidx.core.graphics.createBitmap
import sh.transom.theme.LightPalette

/**
 * A round icon of [size] pixels for a notification: [picture] cropped to a circle, or the initials
 * of [name] on a quiet circle when there is none, as the web shows a party without a picture.
 */
internal fun avatar(picture: Bitmap?, name: String, size: Int): Bitmap {
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
    return icon
}

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
