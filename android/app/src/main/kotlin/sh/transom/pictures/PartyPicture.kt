package sh.transom.pictures

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.ColorMatrix
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * A party's picture, from [url] through [pictures], as a circle [size] across, or the initials of
 * [name] on a quiet circle while it has none, as the web's `.avatar`. A [faded] picture is grey and
 * half clear, as a read row's is, so what is unread stands out above it. The name says who it is
 * beside the picture, so a screen reader skips the picture.
 */
@Composable
fun PartyPicture(
    pictures: Pictures,
    url: String?,
    name: String,
    size: Dp,
    modifier: Modifier = Modifier,
    faded: Boolean = false,
) {
    val pixels = with(LocalDensity.current) { size.roundToPx() }
    val bitmap by produceState<Bitmap?>(null, url, pixels) { value = pictures.bitmap(url, pixels) }
    val shown =
        modifier
            .size(size)
            .clip(CircleShape)
            .clearAndSetSemantics {}
            .let { if (faded) it.alpha(if (isSystemInDarkTheme()) FADED_DARK else FADED) else it }
    val picture = bitmap
    if (picture != null) {
        Image(
            picture.asImageBitmap(),
            contentDescription = null,
            modifier = shown,
            colorFilter =
                if (faded) ColorFilter.colorMatrix(ColorMatrix().apply { setToSaturation(0f) })
                else null,
        )
    } else {
        Box(shown.background(MaterialTheme.colorScheme.surfaceVariant), Alignment.Center) {
            Text(
                initials(name),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                fontSize = if (size < 32.dp) 11.sp else 14.sp,
                fontWeight = FontWeight.SemiBold,
            )
        }
    }
}

/**
 * How clear a read row's picture is, as the web's `--faded`, and fainter in the dark, where a grey
 * picture at the light theme's clarity looks muddy.
 */
private const val FADED = 0.55f
private const val FADED_DARK = 0.45f
