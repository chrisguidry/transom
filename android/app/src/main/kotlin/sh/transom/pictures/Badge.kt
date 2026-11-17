package sh.transom.pictures

import android.graphics.Bitmap
import android.graphics.Paint
import android.graphics.PorterDuff
import android.graphics.PorterDuffXfermode
import androidx.annotation.StringRes
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import sh.transom.R
import sh.transom.exchange.Disposition
import sh.transom.theme.DarkPalette
import sh.transom.theme.LightPalette
import sh.transom.theme.Palette

/**
 * The badge a message's disposition puts on the upper right of its sender's picture, as the web's
 * `.badge`: a problem, a problem that resolved, or a question. An informational message has none.
 * The badge keeps its color on a read row, whose picture is grey, so a list scans by its badges.
 * Its [glyph] tells the badges apart without their colors: SVG path data for a stroke in a box of
 * [BOX] units, the same paths the web draws, from `pages.rs`. The lower right is left to a
 * picture's own mark, and to the app's badge that Android draws over a conversation's icon.
 */
internal enum class Badge(val glyph: String, @StringRes val word: Int) {
    PROBLEM("M6 2.9v3.9M6 9.1h.01", R.string.badge_problem),
    RESOLVED("M3.2 6.3l1.9 1.9 3.7-4.1", R.string.badge_resolved),
    QUESTION(
        "M4.3 4.7a1.75 1.75 0 1 1 2.6 1.5c-.6.35-.9.7-.9 1.2M6 9.5h.01",
        R.string.badge_question,
    );

    /** The badge's color in [palette], as the stylesheet colors each `.badge`. */
    fun color(palette: Palette): Color =
        when (this) {
            PROBLEM -> palette.fire
            RESOLVED -> palette.good
            QUESTION -> palette.accent
        }

    companion object {
        fun of(disposition: Disposition): Badge? =
            when (disposition) {
                Disposition.PROBLEM -> PROBLEM
                Disposition.RESOLVED -> RESOLVED
                Disposition.QUESTION -> QUESTION
                Disposition.INFORMATIONAL -> null
            }
    }
}

/** The side of the box a glyph is drawn in, and the width of its stroke, in the glyph's units. */
private const val BOX = 12f
private const val STROKE = 1.6f

/**
 * [picture] with the badge of [disposition], [size] across, on its upper right, as far outside the
 * picture's corner as the web's.
 */
@Composable
internal fun Badged(disposition: Disposition, size: Dp, picture: @Composable () -> Unit) {
    Box {
        picture()
        Badge.of(disposition)?.let {
            PictureBadge(it, Modifier.align(Alignment.TopEnd).offset(5.dp, (-5).dp).size(size))
        }
    }
}

/**
 * [badge] as a disc of its color with its glyph in the accent's ink, ringed in the paper's color so
 * red still shows on an orange picture. The ring is a ninth of the badge, as the web's 2 pixels of
 * 18. A screen reader says the badge's word.
 */
@Composable
private fun PictureBadge(badge: Badge, modifier: Modifier) {
    val palette = if (isSystemInDarkTheme()) DarkPalette else LightPalette
    val paper = MaterialTheme.colorScheme.surface
    val word = stringResource(badge.word)
    val glyph = remember(badge) { PathParser().parsePathString(badge.glyph).toPath() }
    Canvas(modifier.semantics { contentDescription = word }) {
        val ring = size.width / 9
        val unit = (size.width - 2 * ring) / BOX
        drawCircle(paper)
        drawCircle(badge.color(palette), radius = size.width / 2 - ring)
        translate(ring, ring) {
            scale(unit, Offset.Zero) {
                drawPath(
                    glyph,
                    palette.accentInk,
                    style = Stroke(STROKE, cap = StrokeCap.Round, join = StrokeJoin.Round),
                )
            }
        }
    }
}

/**
 * Draws [badge] on the upper right of this square notification icon, in the web's proportions: 18
 * of the picture's 40 across, with a ring of 2. The ring is cut clear of the picture, so the badge
 * reads against any picture and any shade, and the colors are the light palette's, since the icon
 * is drawn once for both themes.
 */
internal fun Bitmap.drawBadge(badge: Badge) {
    val canvas = android.graphics.Canvas(this)
    val ring = width * 2 / 40f
    val radius = width * 7 / 40f
    val x = width - radius - ring
    val y = radius + ring
    val clear =
        Paint(Paint.ANTI_ALIAS_FLAG).apply { xfermode = PorterDuffXfermode(PorterDuff.Mode.CLEAR) }
    canvas.drawCircle(x, y, radius + ring, clear)
    val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = badge.color(LightPalette).toArgb() }
    canvas.drawCircle(x, y, radius, fill)
    val unit = 2 * radius / BOX
    val stroke =
        Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = LightPalette.accentInk.toArgb()
            style = Paint.Style.STROKE
            strokeWidth = STROKE
            strokeCap = Paint.Cap.ROUND
            strokeJoin = Paint.Join.ROUND
        }
    canvas.translate(x - radius, y - radius)
    canvas.scale(unit, unit)
    canvas.drawPath(androidx.core.graphics.PathParser.createPathFromPathData(badge.glyph), stroke)
}
